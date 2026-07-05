//! Assembly + whole-graph pass — runs after the fused per-file pass (`pipeline::run_file_pass`) has
//! already dropped every parser's AST. Operates on plain `zpz_core` data: `FileArtifact`s -> one
//! tree-wide `CommonIr` -> whole-graph native analyses (circular / unreachable / dead-candidates) ->
//! `merge_findings` with the per-file DSL findings collected during the fused pass.
//!
//! Also runs the optional git-history-dependent analyses: when `EngineConfig::git` is `Some` and `root`
//! is a git repository, `zpz_git::collect` feeds real `FileNode`s (via `zpz_core::build_file_nodes`),
//! from which `zpz_metrics`' `scores`/`health`/`recommendations`/`critical`/`seams` are computed.
//!
//! Two per-file "fragment now, compose later" passes run here over data the fused pass already
//! collected — no second parse: [`late_resolve_cross_file_consumes`] re-resolves a cross-file-indirected
//! `http` CONSUME from merged constant-map fragments, and [`compose_trpc_provides`] merges tRPC router
//! fragments into whole-tree `trpc` PROVIDEs.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use zpz_core::{
    build_file_nodes, circular_from_dep, dsl::RuleTiming, http_interface_key, is_enabled,
    merge_findings, CommonIr, DepGraph, DepStats, FileNode, Finding, GitStats, ImportMap,
    IoConsume, IoFacts, IoProvide, MinimalIr, DEFAULT_WEIGHTS,
};
use zpz_metrics::{
    build_coupling, build_cross_layer_co_churn, build_diagnostics, build_folder_aggregates,
    build_recommendations, compute_criticality, compute_health_index, compute_scores,
    compute_seams, layer_of, scores::types::FileKinds, BuildRecInput, CrossLayerCoChurnOptions,
    DiagnosticsInput, GitDiagnosticsInput, RecommendationGates, ScoresInput, DEFAULT_FOLDER_DEPTH,
};
use zpz_metrics::{
    COUPLING_TOP_PER_FILE, CRITICALITY_LIMIT, CRITICALITY_MIN_BLAST_RADIUS,
    CRITICALITY_SILENT_CHANGE_MAX, SEAMS_LIMIT, SEAMS_MIN_FILES,
};

use crate::pipeline::FileArtifact;
use crate::{AnalyzeOutput, EngineConfig};

/// Consumes the fused pass's per-file artifacts and produces the final `AnalyzeOutput`. `artifacts` must
/// already be sorted by `rel` (an invariant `pipeline::run_file_pass` upholds), which is what makes
/// `ir.ir.symbols` deterministic. `root` is only used for the optional git collection.
pub(crate) fn assemble(
    root: &std::path::Path,
    artifacts: Vec<FileArtifact>,
    config: &EngineConfig,
) -> AnalyzeOutput {
    let file_count = artifacts.len();
    let mut per_file_findings: Vec<Finding> = Vec::new();
    let mut all_symbols = Vec::new();
    let mut loc_by_path: HashMap<String, u32> = HashMap::new();
    let mut ts_import_pairs: Vec<(String, ImportMap)> = Vec::new();
    let mut ts_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut degraded: Vec<String> = Vec::new();
    // `pipeline::eval_packs`' minified/generated skip — a separate list from `degraded` (see
    // `pipeline::FileArtifact::minified_or_generated`'s doc), surfaced as one aggregate `warnings`
    // entry (`minified_files_warning`) rather than a per-file entry.
    let mut minified: Vec<String> = Vec::new();
    let mut io_provides: Vec<IoProvide> = Vec::new();
    let mut io_consumes: Vec<IoConsume> = Vec::new();
    // `dead-exports`' per-file "used names" input — collected unconditionally (cheap, already cached by
    // the fused pass); the `is_enabled` gate below decides whether the more expensive second pass runs.
    let mut used_names_by_file: HashMap<String, Vec<String>> = HashMap::new();
    // `schema-usage`'s whole-tree input: every non-degraded Prisma-dispatched file (a degraded schema
    // parses to zero models, so it's excluded).
    let mut prisma_rels: Vec<String> = Vec::new();
    // `run_java_provides_project_pass`'s whole-corpus input: every java-dispatched file's rel path,
    // collected unconditionally — the project pass needs EVERY java file, not just the ones whose own
    // per-file pass emitted a provide, since a file with no routes of its own (e.g. a prefix-constants
    // file) still needs to be present for its constants to resolve.
    let mut java_rels: Vec<String> = Vec::new();
    // `EngineConfig::profile_rules` reduce step: each `FileArtifact` carries its own file-local
    // `rule_timings`, summed per `rule_id` in the loop below. Stays empty when profiling is off.
    let mut rule_time: HashMap<String, (u128, usize)> = HashMap::new();
    // Per-package (non-relative specifier) importing-file sets — summarized into
    // `AnalyzeOutput::package_imports` for `cross-layer/sdk-import-no-visible-consume` (the tree IR
    // drops package imports during dep resolution, so this is the one place the data still exists).
    let mut package_import_files: std::collections::BTreeMap<
        String,
        std::collections::BTreeSet<String>,
    > = std::collections::BTreeMap::new();
    // Late consume resolution's substrate: each TS file's own constant-map fragment, paired with its
    // `rel` so the merge below can sort by path for deterministic first-writer-wins resolution of a key
    // duplicated across files.
    let mut fragment_pairs: Vec<(String, HashMap<String, String>)> = Vec::new();
    // tRPC PROVIDE composition's substrate (`compose_trpc_provides`): each TS file's own tRPC
    // router-fragment shape, paired with its `rel`. Composed directly into `IoProvide`s rather than
    // re-keying an `IoConsume` (see `crate::io`'s module doc).
    let mut trpc_fragment_pairs: Vec<(String, Vec<zpz_core::TrpcRouterFragment>)> = Vec::new();
    // Code-registered router-mount composition's substrate (`compose_router_mount_provides`): the
    // provide-side sibling of `trpc_fragment_pairs`, for Hono-style chained builders and cross-file
    // sub-router mounts.
    let mut router_mount_pairs: Vec<(String, Vec<zpz_core::RouterMountFragment>)> = Vec::new();
    // Wrapper-consume join's substrate (`resolve_wrapper_consumes`): per-file wrapper DEFINITION
    // fragments (exported fns whose signature carries method/path params and whose body reaches an
    // HTTP sink) and wrapper CALL fragments (call sites with captured literal args). The join
    // re-anchors HTTP consumes from wrapper internals (where egress sees only a non-literal
    // `axios.request(opts)`) to the real FE call sites.
    let mut wrapper_def_pairs: Vec<(String, Vec<zpz_core::WrapperDefFragment>)> = Vec::new();
    let mut wrapper_call_pairs: Vec<(String, Vec<zpz_core::WrapperCallFragment>)> = Vec::new();

    for artifact in artifacts {
        loc_by_path.insert(artifact.rel.clone(), artifact.loc);
        if artifact.minified_or_generated {
            minified.push(artifact.rel.clone());
        }
        if artifact.degraded {
            degraded.push(artifact.rel.clone());
        } else if crate::dispatch::dispatch(&artifact.rel, &config.dispatch)
            == Some(crate::dispatch::Language::Prisma)
        {
            prisma_rels.push(artifact.rel.clone());
        } else if crate::dispatch::dispatch(&artifact.rel, &config.dispatch)
            == Some(crate::dispatch::Language::JavaLexical)
        {
            java_rels.push(artifact.rel.clone());
        }
        if let Some(imports) = artifact.imports {
            for binding in imports.values() {
                if !binding.specifier.starts_with('.') && !binding.specifier.starts_with('/') {
                    package_import_files
                        .entry(binding.specifier.clone())
                        .or_default()
                        .insert(artifact.rel.clone());
                }
            }
            ts_paths.insert(artifact.rel.clone());
            ts_import_pairs.push((artifact.rel.clone(), imports));
            used_names_by_file.insert(artifact.rel.clone(), artifact.used_names.clone());
        }
        if let Some(io) = artifact.io {
            io_provides.extend(io.provides);
            io_consumes.extend(io.consumes);
        }
        if !artifact.const_map_fragment.is_empty() {
            fragment_pairs.push((artifact.rel.clone(), artifact.const_map_fragment));
        }
        if !artifact.trpc_router_fragments.is_empty() {
            trpc_fragment_pairs.push((artifact.rel.clone(), artifact.trpc_router_fragments));
        }
        if !artifact.router_mount_fragments.is_empty() {
            router_mount_pairs.push((artifact.rel.clone(), artifact.router_mount_fragments));
        }
        if !artifact.wrapper_def_fragments.is_empty() {
            wrapper_def_pairs.push((artifact.rel.clone(), artifact.wrapper_def_fragments));
        }
        if !artifact.wrapper_call_fragments.is_empty() {
            wrapper_call_pairs.push((artifact.rel.clone(), artifact.wrapper_call_fragments));
        }
        all_symbols.extend(artifact.symbols);
        for t in artifact.rule_timings {
            let entry = rule_time.entry(t.rule_id).or_insert((0, 0));
            entry.0 += t.nanos;
            entry.1 += t.findings;
        }
        per_file_findings.extend(artifact.findings);
    }

    late_resolve_cross_file_consumes(fragment_pairs, &mut io_consumes);

    // Whole-corpus Java Spring HTTP-provides resolution — a no-op when `java_rels` is empty.
    if !java_rels.is_empty() {
        run_java_provides_project_pass(root, &java_rels, &mut io_provides);
    }

    // Workspace-package manifest scan — hoisted above `build_dep` because `workspace_pkgs` also feeds
    // cross-package import resolution (`build_dep_with_workspace`): a monorepo import like
    // `import { x } from '@scope/pkg-b'` names a workspace package, not an npm dependency, and every
    // whole-graph analysis downstream of `dep` needs that edge to exist.
    let pkg_scan =
        crate::pipeline::package_json_entries(root, loc_by_path.keys().cloned(), &ts_paths);
    // tsconfig `paths`/`baseUrl` alias collection: a monorepo import like `import { x } from
    // '@/features/y'` remapped by `compilerOptions.paths` needs this to become a real dep-graph edge
    // instead of looking external/orphaned.
    let tsconfigs = crate::pipeline::tsconfig_scan(root, loc_by_path.keys().cloned());

    // tRPC PROVIDE composition — must run after `pkg_scan`/`tsconfigs` exist (a `Ref`'s import specifier
    // resolves through the same workspace/tsconfig-aware resolver as dep-graph edges) and while
    // `io_provides` is still mutable. Exact-rel-first, same as the router-mount composer below: the
    // envelope fragment contract lets an external adapter's `Ref` specifier be the target file's rel
    // path verbatim, so a fully-external-language overlay tree (no `ts_paths` entries at all) still
    // resolves cross-file mounts.
    if !trpc_fragment_pairs.is_empty() {
        let composed = compose_trpc_provides(trpc_fragment_pairs, |specifier, from_file| {
            if loc_by_path.contains_key(specifier) {
                return Some(specifier.to_string());
            }
            zpz_parser_typescript::resolve_file_with_workspace(
                specifier,
                from_file,
                &ts_paths,
                &pkg_scan.workspace_pkgs,
                &tsconfigs,
            )
        });
        io_provides.extend(composed);
    }

    // Code-registered router PROVIDE composition — the provide-side twin of the tRPC block just
    // above, over `router_mount_fragments` (Hono-style chained builders + cross-file sub-router
    // mounts). Same placement constraints, and the same exact-rel-first resolver: the per-file pass
    // emits no code-registered router provides of its own, so this composition is the single source
    // of truth (no retain/dedup against per-file output needed).
    if !router_mount_pairs.is_empty() {
        let composed = compose_router_mount_provides(router_mount_pairs, |specifier, from_file| {
            if loc_by_path.contains_key(specifier) {
                return Some(specifier.to_string());
            }
            zpz_parser_typescript::resolve_file_with_workspace(
                specifier,
                from_file,
                &ts_paths,
                &pkg_scan.workspace_pkgs,
                &tsconfigs,
            )
        });
        io_provides.extend(composed);
    }

    // Wrapper-consume join — re-anchors HTTP consumes from wrapper internals to real FE call sites
    // (`resolve_wrapper_consumes`'s own doc). Same placement constraints as the provide composers
    // above: needs the workspace resolver, and must run while `io_consumes` is still mutable.
    if !wrapper_call_pairs.is_empty() && !wrapper_def_pairs.is_empty() {
        resolve_wrapper_consumes(
            wrapper_def_pairs,
            wrapper_call_pairs,
            |specifier, from_file| {
                zpz_parser_typescript::resolve_file_with_workspace(
                    specifier,
                    from_file,
                    &ts_paths,
                    &pkg_scan.workspace_pkgs,
                    &tsconfigs,
                )
            },
            &mut io_consumes,
        );
    }

    // File-convention route PROVIDE composition — frameworks whose HTTP surface is the file tree
    // itself (Next.js `pages/api` + app-router `route.ts`, Remix flat routes, Medusa-style
    // `src/api/**/route.ts`). Pure path+symbol logic over `all_symbols`/`loc_by_path`; `pages/api`
    // candidates alone are re-read from disk for a lexical default-export/verb scan. See
    // `file_routes`'s module doc for the v1 scope decisions.
    {
        let composed = crate::file_routes::compose_file_convention_provides(
            loc_by_path.keys().map(String::as_str),
            &all_symbols,
            &|rel| std::fs::read_to_string(root.join(rel)).ok(),
        );
        io_provides.extend(composed);
    }

    let dep: DepGraph = zpz_parser_typescript::build_dep_with_workspace(
        &ts_import_pairs,
        &ts_paths,
        &pkg_scan.workspace_pkgs,
        &tsconfigs,
    );
    let cycles = circular_from_dep(&dep);

    let dep_stats = dep_stats_from_dep(&dep);

    // Git-history-dependent analyses. `None`/failed-collection both fall through to a default
    // (all-zero) `GitStats` and no commits — `nodes` still builds (dep-graph + LOC signal only) and
    // scores/health/recommendations/critical/seams stay empty.
    let mut warnings: Vec<String> = Vec::new();
    if let Some(w) = git_not_requested_warning(config) {
        warnings.push(w);
    }
    if let Some(w) = zero_packs_warning(config) {
        warnings.push(w);
    }
    let (git_stats, commits, git_active) = collect_git(root, config, &mut warnings);

    // `is_source`: reuses the same dispatch classification the fused pass used to pick a parser
    // frontend, so `risk_score`/`hotspot_score` are zeroed for non-source files (data/config/assets)
    // right where `FileNode`s are built.
    let is_source = |id: &str| crate::dispatch::dispatch(id, &config.dispatch).is_some();
    let nodes = build_file_nodes(
        &dep_stats,
        &git_stats,
        &loc_by_path,
        &DEFAULT_WEIGHTS,
        is_source,
    );

    // `AnalyzeOutput::folders` is not git-gated: `nodes`/`dep` are both already built unconditionally.
    let folders = Some(build_folder_aggregates(&nodes, &dep, DEFAULT_FOLDER_DEPTH));

    // `profile` mirrors `dsl::eval_pack_impl`'s no-op-sink convention: `Instant::now()` is only ever called
    // when profiling is on, so a non-profiled `analyze_tree` call pays zero cost for the wrapping below.
    let profile = config.profile_rules;
    let mut global_findings = Vec::new();
    if is_enabled(&config.rule_config, "circular") {
        let t0 = profile.then(Instant::now);
        let found = circular_findings(&cycles);
        record_native_timing(&mut rule_time, t0, "circular", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "unreachable") {
        let t0 = profile.then(Instant::now);
        let found = unreachable_findings(&nodes, &dep);
        record_native_timing(&mut rule_time, t0, "unreachable", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "dead-candidates") {
        // `extra_entries`: package.json-referenced files (manifest entry fields + lexically-scanned
        // `scripts` path tokens) — real entry points loaded by Node/bundlers/npm directly, never via
        // `import`, so `fan_in == 0` on them is expected, not dead-code signal.
        let t0 = profile.then(Instant::now);
        let found = dead_candidate_findings(&nodes, &dep, &pkg_scan.extra_entries);
        record_native_timing(&mut rule_time, t0, "dead-candidates", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "dead-exports") {
        let t0 = profile.then(Instant::now);
        let found = crate::dead_exports::dead_export_findings(
            root,
            &ts_paths,
            &ts_import_pairs,
            &all_symbols,
            &used_names_by_file,
            &pkg_scan.workspace_pkgs,
            &tsconfigs,
        );
        record_native_timing(&mut rule_time, t0, "dead-exports", found.len());
        global_findings.extend(found);
    }

    if is_enabled(&config.rule_config, "schema-usage") {
        let t0 = profile.then(Instant::now);
        let found = crate::pipeline::schema_usage_findings(root, &prisma_rels);
        record_native_timing(&mut rule_time, t0, "schema-usage", found.len());
        global_findings.extend(found);
    }

    // The schema x usage JOIN native rules — see `run_schema_join_rules`'s own doc.
    run_schema_join_rules(
        root,
        &prisma_rels,
        config,
        profile,
        &mut rule_time,
        &mut global_findings,
    );

    // Native fullstack rule: same (METHOD, path) HTTP route provided 2+ times across the tree — a
    // whole-tree pass over `io_provides` already collected above.
    if is_enabled(&config.rule_config, "duplicate-route") {
        let t0 = profile.then(Instant::now);
        let found = zpz_rules_graph::duplicate_route_findings(&io_provides);
        record_native_timing(&mut rule_time, t0, "duplicate-route", found.len());
        global_findings.extend(found);
    }

    // Native fullstack rule: within one file, an earlier param route shadows a later literal route of
    // the same shape (see `zpz_rules_graph::route_shadowing`'s module doc for the decidable subset).
    if is_enabled(&config.rule_config, "route-shadowing") {
        let t0 = profile.then(Instant::now);
        let found = zpz_rules_graph::route_shadowing_findings(&io_provides);
        record_native_timing(&mut rule_time, t0, "route-shadowing", found.len());
        global_findings.extend(found);
    }

    // Native fullstack rule: a resolved `http` consume with no matching provide anywhere in this tree,
    // gated on this tree itself having at least one `http` provide (see
    // `zpz_rules_graph::unprovided_consume`'s module doc for the zero-provides veto).
    if is_enabled(&config.rule_config, "unprovided-consume") {
        let t0 = profile.then(Instant::now);
        let found = zpz_rules_graph::unprovided_consume_findings(&io_provides, &io_consumes);
        record_native_timing(&mut rule_time, t0, "unprovided-consume", found.len());
        global_findings.extend(found);
    }

    run_callgraph_rules(
        root,
        config,
        &io_provides,
        &ts_paths,
        &ts_import_pairs,
        &all_symbols,
        profile,
        &mut rule_time,
        &mut global_findings,
    );

    let findings = merge_findings(
        vec![per_file_findings, global_findings],
        &config.rule_config,
    );

    degraded.sort();
    minified.sort();
    if let Some(w) = minified_files_warning(&minified) {
        warnings.push(w);
    }
    io_provides.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    io_consumes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });

    // BE-framework coverage self-report (`crate::framework_silence`): flags a tree that looks like it
    // has a backend but produced zero `http` provides — an unsupported/unrecognized framework signal.
    // Computed here, while `io_provides`/`ts_paths`/`java_rels` are still in scope.
    let http_count = io_provides.iter().filter(|p| p.kind == "http").count();
    let mut candidate_rels: Vec<String> = ts_paths.iter().cloned().collect();
    candidate_rels.extend(java_rels.iter().cloned());
    candidate_rels.sort();
    candidate_rels.dedup();
    if let Some(w) =
        crate::framework_silence::controller_silence_warning(root, &candidate_rels, http_count)
    {
        warnings.push(w);
    }

    let io = if io_provides.is_empty() && io_consumes.is_empty() {
        None
    } else {
        Some(IoFacts {
            provides: io_provides,
            consumes: io_consumes,
        })
    };

    let (scores, health, recommendations, critical, seams) = if git_active {
        let coupling = build_coupling(&commits, COUPLING_TOP_PER_FILE);

        let t0 = profile.then(Instant::now);
        let scores = compute_scores(
            &ScoresInput {
                nodes: &nodes,
                dep: &dep,
                circular: &cycles,
                target: None,
                file_kinds: &FileKinds::new(),
                type_safety_counts: &HashMap::new(),
                lod_by_file: &HashMap::new(),
                is_source: &is_source,
            },
            &config.scores_config,
        );
        // `scores`/`health` produce one struct, not a `Vec` — `findings: 0` is the convention for a
        // native analysis id with nothing list-shaped to count.
        record_native_timing(&mut rule_time, t0, "scores", 0);

        let t0 = profile.then(Instant::now);
        let health = compute_health_index(&scores);
        record_native_timing(&mut rule_time, t0, "health", 0);

        let t0 = profile.then(Instant::now);
        let recommendations = build_recommendations(
            &BuildRecInput {
                nodes: &nodes,
                dep: &dep,
                coupling: &coupling,
                circular: &cycles,
                scope_excludes: &[],
                permanent_ignores: &[],
                untested_paths: &HashSet::new(),
                amplification_by_path: &HashMap::new(),
                findings: &findings,
            },
            &RecommendationGates::default(),
        );
        record_native_timing(&mut rule_time, t0, "recommendations", recommendations.len());

        let t0 = profile.then(Instant::now);
        let critical = compute_criticality(
            &nodes,
            &dep,
            CRITICALITY_MIN_BLAST_RADIUS,
            CRITICALITY_SILENT_CHANGE_MAX,
            CRITICALITY_LIMIT,
        );
        record_native_timing(&mut rule_time, t0, "criticality", critical.len());

        let t0 = profile.then(Instant::now);
        let seams = compute_seams(&dep, &coupling, SEAMS_MIN_FILES, SEAMS_LIMIT);
        record_native_timing(&mut rule_time, t0, "seams", seams.len());

        (Some(scores), Some(health), recommendations, critical, seams)
    } else {
        (None, None, Vec::new(), Vec::new(), Vec::new())
    };

    // `AnalyzeOutput::layer_co_churn` — git-gated like `scores`/`health` above: `None` when git is
    // inactive, `Some` (possibly an empty `Vec`) when it succeeded. `layer_of` folds
    // `hierarchy_shared_dirs` into a shared, non-layer sentinel.
    let layer_co_churn = git_active.then(|| {
        build_cross_layer_co_churn(
            &commits,
            |p| layer_of(p, &config.scores_config.hierarchy_shared_dirs),
            &CrossLayerCoChurnOptions::default(),
        )
    });

    warnings.extend(run_diagnostics(
        file_count,
        &dep,
        &all_symbols,
        &commits,
        config,
        git_active,
    ));

    let rule_timings = profile.then(|| sort_rule_timings(rule_time));

    let ir = CommonIr {
        source: config.source_id.clone(),
        // Multiple parser frontends (TypeScript + Prisma, v1 scope) are fused into one tree-wide IR here —
        // no single `parser` id is accurate the way it is for a single-frontend `build_common_ir` call, so
        // this is a zpz-only tag naming the fused engine itself rather than one frontend.
        parser: "engine".to_string(),
        ir: MinimalIr {
            dep,
            symbols: all_symbols,
            loc: loc_by_path,
            io,
        },
    };

    let package_imports = package_import_files
        .into_iter()
        .map(|(specifier, files)| crate::PackageImportSummary {
            file_count: files.len(),
            // BTreeSet iteration is sorted — first() is the lexicographically first importing file.
            example_file: files.into_iter().next().unwrap_or_default(),
            specifier,
        })
        .collect();

    AnalyzeOutput {
        ir,
        findings,
        degraded,
        file_count,
        package_imports,
        nodes,
        scores,
        health,
        recommendations,
        critical,
        seams,
        folders,
        layer_co_churn,
        warnings,
        // Set by `analyze_tree` after this call returns (needs the counters that `pipeline::run_file_pass`
        // updated during the fused pass, which are private to that call, not `assemble`'s).
        cache: None,
        rule_timings,
    }
}

/// Late cross-file constant re-resolution — closes the gap `crate::io`'s module doc documents as the "v1
/// fusion tradeoff": a one-file-slice HTTP egress scan cannot resolve a constant imported from another
/// file, so it emits `IoConsume { key: None, raw: Some(<dotted expr text>), method: Some(<METHOD>) }`
/// instead of guessing. This function fixes that up AFTER every file's own constant-map fragment has
/// been collected, using only data the fused per-file pass already produced — no second parse.
///
/// **Deterministic merge**: `fragments` is sorted by `rel`, then folded first-writer-wins: a constant
/// key duplicated across two files always resolves to the lexicographically smallest file's value,
/// independent of `HashMap`/rayon iteration order.
///
/// **Re-resolution**: every consume with `key: None` whose `raw`/`method` are both `Some` is looked up
/// via `zpz_parser_typescript::resolve_raw_path`; a hit sets `key` to the normalized join key and
/// deliberately keeps `raw` as provenance (this consume was only resolvable via the project-wide
/// constant merge, not from its own file alone). A miss leaves the consume exactly as unresolved as
/// before — this function only ever turns an unresolved consume INTO a resolved one, never the reverse.
///
/// Must run before `io_consumes` is frozen into `MinimalIr::io` — every whole-tree native rule that
/// reads `io_consumes` directly must see the resolved key, not the raw one.
pub(crate) fn late_resolve_cross_file_consumes(
    mut fragments: Vec<(String, HashMap<String, String>)>,
    io_consumes: &mut [IoConsume],
) {
    fragments.sort_by(|a, b| a.0.cmp(&b.0));
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    for (_, fragment) in fragments {
        for (key, value) in fragment {
            merged.entry(key).or_insert(value);
        }
    }
    // `resolve_raw_path` takes `&HashMap` — the `BTreeMap` above exists only so the merge loop itself is
    // deterministic; the lookup below has no ordering requirement of its own.
    let consts: HashMap<String, String> = merged.into_iter().collect();
    for consume in io_consumes.iter_mut() {
        if consume.key.is_some() {
            continue;
        }
        let (Some(raw), Some(method)) = (consume.raw.as_deref(), consume.method.as_deref()) else {
            continue;
        };
        if let Some(path) = zpz_parser_typescript::resolve_raw_path(raw, &consts) {
            // A leading `/` is an internal route (normalized key); an absolute `http(s)://` URL keeps
            // the verbatim host-carrying key so `link_cross_layer_io`'s `"://"` gate still routes it
            // to the `external` bucket; anything else — a bare fragment — stays unresolved.
            if path.starts_with('/') {
                consume.key = Some(http_interface_key(method, &path));
            } else if zpz_parser_typescript::is_external_url(&path) {
                consume.key = Some(format!("{method} {path}"));
            }
        }
    }
}

/// Composes every file's tRPC router fragment into whole-tree `IoProvide`s — the assembly-time
/// counterpart of `late_resolve_cross_file_consumes` for the `trpc` kind, except here the cross-file
/// join produces brand-new PROVIDEs directly rather than re-keying an already-emitted CONSUME: a leaf's
/// full dotted route path is often only knowable once every file's fragment is assembled together.
///
/// `resolve` is `(specifier, from_file) -> Option<target_rel>` — the caller passes a closure over
/// `zpz_parser_typescript::resolve_file_with_workspace` (the same resolver `assemble` uses for TS
/// dep-graph edges) so this function itself stays a pure, filesystem-free composition — easy to unit
/// test with a hand-built resolver map.
///
/// ## Resolution
/// Fragments are indexed by `(rel, name)`. Each `TrpcRouterEntry::Ref` is resolved to a target fragment
/// key: `specifier: Some(s)` -> `resolve(s, rel)` gives the target file, then `(target_rel, ident)`;
/// `specifier: None` -> same-file, `(rel, ident)`. A `Ref` whose specifier does not resolve, or whose
/// resolved key names no known fragment, is skipped — honest absence, never fabricated.
///
/// ## Roots and composition
/// A fragment is a ROOT when no resolved `Ref` anywhere in the corpus targets it — composition starts
/// from every root (BTree-ordered for determinism) and walks each fragment's entries depth-first:
/// `Nested` appends its `key` to the current dotted path; `Ref` with a non-empty `key` appends `key`
/// then recurses into the target fragment; `Ref` with an empty `key` (a `mergeRouters(...)` argument)
/// splices the target's entries in at the current path, adding no segment; `Leaf` emits one `IoProvide`
/// (`file`/`line` from the leaf's own originating fragment, which after a `Ref` hop is the target
/// fragment's file, not the file containing the `Ref`). An `ancestry` stack guards against a cyclic
/// `Ref` chain — a fragment already on the stack is skipped rather than recursed into again.
///
/// Deduped on `(kind, key, file, line)` and sorted to match the ordering `assemble` applies to every
/// other `IoProvide` before freezing `MinimalIr::io`.
pub(crate) fn compose_trpc_provides(
    fragments: Vec<(String, Vec<zpz_core::TrpcRouterFragment>)>,
    resolve: impl Fn(&str, &str) -> Option<String>,
) -> Vec<IoProvide> {
    use zpz_core::{TrpcRouterEntry, TrpcRouterFragment};

    let mut by_key: HashMap<(String, String), &TrpcRouterFragment> = HashMap::new();
    for (rel, frags) in &fragments {
        for frag in frags {
            by_key.insert((rel.clone(), frag.name.clone()), frag);
        }
    }

    // Resolves one `Ref`'s target fragment key, honoring the `specifier: Some -> resolve` / `specifier:
    // None -> same-file` split — shared by both the "which fragments are targeted" pass below and the
    // actual composition walk.
    let ref_target = |origin_rel: &str, ident: &str, specifier: &Option<String>| {
        let target_rel = match specifier {
            Some(s) => resolve(s, origin_rel)?,
            None => origin_rel.to_string(),
        };
        let key = (target_rel, ident.to_string());
        by_key.contains_key(&key).then_some(key)
    };

    // Every fragment key targeted by at least one resolved `Ref`, anywhere — plus, conservatively, every
    // ident ANY `Ref` names (resolved or not): a fragment whose name some mount references is intended
    // as a sub-router somewhere, so promoting it to a root when the mount's specifier failed to resolve
    // would emit its leaves under a truncated path (e.g. bare `createLicense` instead of
    // `viewer.admin.createLicense`) — a mis-keyed provide. Skipping it entirely is the honest-absence
    // choice; the cost is missing a genuinely independent root that merely shares its name with some ref
    // ident — rare, and an under- rather than over-report.
    let mut targeted: HashSet<(String, String)> = HashSet::new();
    let mut ref_named_idents: HashSet<String> = HashSet::new();
    fn collect_targeted(
        entries: &[TrpcRouterEntry],
        origin_rel: &str,
        ref_target: &impl Fn(&str, &str, &Option<String>) -> Option<(String, String)>,
        targeted: &mut HashSet<(String, String)>,
        ref_named_idents: &mut HashSet<String>,
    ) {
        for entry in entries {
            match entry {
                TrpcRouterEntry::Ref {
                    ident, specifier, ..
                } => {
                    ref_named_idents.insert(ident.clone());
                    if let Some(key) = ref_target(origin_rel, ident, specifier) {
                        targeted.insert(key);
                    }
                }
                TrpcRouterEntry::Nested { entries, .. } => {
                    collect_targeted(entries, origin_rel, ref_target, targeted, ref_named_idents);
                }
                TrpcRouterEntry::Leaf { .. } => {}
            }
        }
    }
    for (rel, frags) in &fragments {
        for frag in frags {
            collect_targeted(
                &frag.entries,
                rel,
                &ref_target,
                &mut targeted,
                &mut ref_named_idents,
            );
        }
    }

    let mut roots: Vec<(String, String)> = by_key
        .keys()
        .filter(|k| !targeted.contains(*k) && !ref_named_idents.contains(&k.1))
        .cloned()
        .collect();
    roots.sort();

    #[allow(clippy::too_many_arguments)]
    fn compose_entries(
        entries: &[TrpcRouterEntry],
        origin_rel: &str,
        path: &[String],
        by_key: &HashMap<(String, String), &TrpcRouterFragment>,
        ref_target: &impl Fn(&str, &str, &Option<String>) -> Option<(String, String)>,
        ancestry: &mut Vec<(String, String)>,
        out: &mut Vec<IoProvide>,
    ) {
        for entry in entries {
            match entry {
                TrpcRouterEntry::Leaf { key, verb, line } => {
                    let full_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{key}", path.join("."))
                    };
                    out.push(IoProvide {
                        kind: "trpc".to_string(),
                        key: format!("{verb} {full_path}"),
                        file: origin_rel.to_string(),
                        line: *line,
                        symbol: None,
                    });
                }
                TrpcRouterEntry::Nested {
                    key,
                    entries: inner,
                } => {
                    let mut new_path = path.to_vec();
                    new_path.push(key.clone());
                    compose_entries(
                        inner, origin_rel, &new_path, by_key, ref_target, ancestry, out,
                    );
                }
                TrpcRouterEntry::Ref {
                    key,
                    ident,
                    specifier,
                } => {
                    let Some(target_key) = ref_target(origin_rel, ident, specifier) else {
                        continue; // unresolvable specifier, or no fragment named `ident` there — skip, never guess
                    };
                    if ancestry.contains(&target_key) {
                        continue; // cycle guard
                    }
                    let target_frag = by_key[&target_key];
                    let new_path = if key.is_empty() {
                        path.to_vec() // mergeRouters splice-in-place — no path segment added
                    } else {
                        let mut p = path.to_vec();
                        p.push(key.clone());
                        p
                    };
                    ancestry.push(target_key.clone());
                    compose_entries(
                        &target_frag.entries,
                        &target_key.0,
                        &new_path,
                        by_key,
                        ref_target,
                        ancestry,
                        out,
                    );
                    ancestry.pop();
                }
            }
        }
    }

    let mut out = Vec::new();
    for root_key in roots {
        let frag = by_key[&root_key];
        let mut ancestry = vec![root_key.clone()];
        compose_entries(
            &frag.entries,
            &root_key.0,
            &[],
            &by_key,
            &ref_target,
            &mut ancestry,
            &mut out,
        );
    }

    let mut seen: HashSet<(String, String, String, u32)> = HashSet::new();
    out.retain(|p| seen.insert((p.kind.clone(), p.key.clone(), p.file.clone(), p.line)));
    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out
}

/// Times one whole-graph native analysis (`EngineConfig::profile_rules`): `t0` is `Some` exactly when
/// profiling is on, so the caller never pays an `Instant::now()` otherwise. Native analysis ids never
/// collide with DSL `rule_id`s (always pack-prefixed with a `/`), so keying both into the same
/// `HashMap` is safe.
fn record_native_timing(
    rule_time: &mut HashMap<String, (u128, usize)>,
    t0: Option<Instant>,
    id: &str,
    findings: usize,
) {
    let Some(t0) = t0 else { return };
    let entry = rule_time.entry(id.to_string()).or_insert((0, 0));
    entry.0 += t0.elapsed().as_nanos();
    entry.1 += findings;
}

/// Finalizes the accumulated per-rule timings into `AnalyzeOutput::rule_timings`'s documented order:
/// `nanos` descending, `rule_id` ascending tie-break — deterministic regardless of `HashMap` iteration
/// order or rayon per-file scheduling.
fn sort_rule_timings(rule_time: HashMap<String, (u128, usize)>) -> Vec<RuleTiming> {
    let mut out: Vec<RuleTiming> = rule_time
        .into_iter()
        .map(|(rule_id, (nanos, findings))| RuleTiming {
            rule_id,
            nanos,
            findings,
        })
        .collect();
    out.sort_by(|a, b| {
        b.nanos
            .cmp(&a.nanos)
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });
    out
}

/// Runs `zpz_git::collect` when `config.git` is `Some`, pushing a warning (never panicking, never
/// failing the analysis) when `root` is not a git repository / `git` is unavailable / collection
/// otherwise fails. Returns `(GitStats::default(), vec![], false)` for every "not active" case so the
/// caller's git-dependent computations can gate on the returned `bool` alone.
fn collect_git(
    root: &std::path::Path,
    config: &EngineConfig,
    warnings: &mut Vec<String>,
) -> (GitStats, Vec<zpz_core::CommitFileSet>, bool) {
    let Some(git_opts) = &config.git else {
        return (GitStats::default(), Vec::new(), false);
    };
    let opts = zpz_git::CollectOptions {
        since: git_opts.since.clone(),
        recent_days: git_opts.recent_days,
        // The default FIX/FEAT/... keyword vocabulary is analysis-domain, not collection-mechanism, so
        // it lives in `zpz-metrics` rather than `zpz-git` — collector crates own the mechanism, not the
        // domain vocabulary.
        commit_type_patterns: zpz_metrics::default_commit_type_patterns(),
    };
    match zpz_git::collect(root, &opts) {
        Ok(collection) => (collection.stats, collection.commits, true),
        Err(e) => {
            warnings.push(format!(
                "git collection skipped for {}: {e}",
                root.display()
            ));
            (GitStats::default(), Vec::new(), false)
        }
    }
}

/// Builds `zpz_metrics::diagnostics`' coverage-gap self-report from data `assemble` already has in
/// scope — no extra pass. `symbols` filters on `SourceSymbol::exported` since `all_symbols` also
/// carries unexported top-level declarations. `concrete_modules`/`total_modules` are always `0` — no
/// real module classification is wired at this call site yet, and `0`/`0` is the honest "not measured"
/// value (the module's own `total_modules > 1` guard means that pair simply never fires until it is).
///
/// **Git-disabled gating**: `DiagnosticsInput::git` is `Option<GitDiagnosticsInput>` so the module
/// itself can tell "git was never attempted" (`None`) apart from "git ran and found zero" (`Some` with
/// honest zero counts) — `build_diagnostics` skips every git-window warning when `git` is `None`. This
/// passes `None` when `git_active` is `false`, `Some` with the honest counts otherwise.
fn run_diagnostics(
    file_count: usize,
    dep: &DepGraph,
    symbols: &[zpz_core::SourceSymbol],
    commits: &[zpz_core::CommitFileSet],
    config: &EngineConfig,
    git_active: bool,
) -> Vec<String> {
    let dep_edges: u32 = dep.values().map(|targets| targets.len() as u32).sum();
    let exported_symbols = symbols.iter().filter(|s| s.exported).count() as u32;

    let git = git_active.then(|| {
        let (total_changes, tagged_changes, fix_changes) =
            commits
                .iter()
                .fold((0u32, 0u32, 0u32), |(total, tagged, fix), c| {
                    let n = c.files.len() as u32;
                    let tagged = tagged + if c.tags.is_empty() { 0 } else { n };
                    let fix = fix
                        + if c.tags.iter().any(|t| t == "FIX") {
                            n
                        } else {
                            0
                        };
                    (total + n, tagged, fix)
                });
        GitDiagnosticsInput {
            total_changes,
            tagged_changes,
            fix_changes,
            commits: commits.len() as u32,
            since: config.git.as_ref().and_then(|g| g.since.clone()),
        }
    });

    let diagnostics = build_diagnostics(DiagnosticsInput {
        files: file_count as u32,
        dep_edges,
        symbols: exported_symbols,
        concrete_modules: 0,
        total_modules: 0,
        git,
        unknown_disabled_rule_ids: unknown_disabled_rule_ids(config),
    });

    diagnostics.warnings
}

/// Capability self-report: git history was never requested (`config.git` is `None`), so every
/// git-derived output channel is null. Distinct from `collect_git`'s own warning, which fires only when
/// git WAS requested but collection failed — a consumer can always tell "never asked" apart from
/// "asked, failed" by which of the two strings is present. Returns `None` when git was requested.
fn git_not_requested_warning(config: &EngineConfig) -> Option<String> {
    if config.git.is_some() {
        return None;
    }
    Some(
        "git history not requested (git option omitted): scores, health, recommendations, criticality, seams and layerCoChurn are null. Pass git: {} to enable them."
            .to_string(),
    )
}

/// Capability self-report: no DSL rule packs are loaded (`config.packs` is empty), so only the built-in
/// native analyses ran. `pub(crate)` because it is shared between `assemble` and
/// `envelope::analyze_envelope`, which gate DSL packs identically on `config.packs`. Per this codebase's
/// kernel-agnostic-no-rule-data principle the message names no rule/vocab, only the native-analysis
/// count and the `packsDir` config hint. Returns `None` when at least one pack is loaded.
pub(crate) fn zero_packs_warning(config: &EngineConfig) -> Option<String> {
    if !config.packs.is_empty() {
        return None;
    }
    let mut registry = zpz_core::RuleRegistry::new();
    crate::register_all_native(&mut registry);
    let native_count = registry.metas().len();
    Some(format!(
        "no DSL rule packs loaded: only the {native_count} built-in native analyses ran. Set packsDir to a directory of *.json rule packs to enable the shipped DSL rules."
    ))
}

/// Capability self-report: how many files this run classified minified/generated and were therefore
/// skipped for every DSL rule-pack matcher type (distinct from `degraded`, which still runs line-scan
/// rules). One aggregate entry, never one per file. `sorted_rels` must already be sorted. Returns `None`
/// when nothing was skipped this way.
fn minified_files_warning(sorted_rels: &[String]) -> Option<String> {
    if sorted_rels.is_empty() {
        return None;
    }
    const SAMPLE: usize = 3;
    let sample: Vec<&str> = sorted_rels
        .iter()
        .take(SAMPLE)
        .map(String::as_str)
        .collect();
    let mut sample_str = sample.join(", ");
    if sorted_rels.len() > SAMPLE {
        sample_str.push_str(&format!(", +{} more", sorted_rels.len() - SAMPLE));
    }
    Some(format!(
        "{} minified/generated file(s) skipped for ALL DSL rule-pack rules (long-line-dominated or 5000+ byte single lines; native structural analyses still cover them): {sample_str}",
        sorted_rels.len()
    ))
}

/// `RuleConfig::disabled_rules` entries that match no known rule id — the substrate for
/// `DiagnosticsInput::unknown_disabled_rule_ids`. "Known" is the union of every native-analysis id
/// (built fresh here since the engine keeps no live `RuleRegistry` of its own), every `config.packs`
/// pack id, and every `"<pack>/<rule>"` id within those packs.
fn unknown_disabled_rule_ids(config: &EngineConfig) -> Vec<String> {
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut registry = zpz_core::RuleRegistry::new();
    crate::register_all_native(&mut registry);
    known.extend(registry.metas().iter().map(|m| m.id.clone()));
    for pack in &config.packs {
        known.insert(pack.id.clone());
        for rule in &pack.rules {
            known.insert(format!("{}/{}", pack.id, rule.id));
        }
    }
    config
        .rule_config
        .disabled_rules
        .iter()
        .filter(|id| !known.contains(id.as_str()))
        .cloned()
        .collect()
}

/// Fan-in/fan-out/all-paths derived from a resolved dep graph — the minimal `DepStats`-shaped input
/// `build_file_nodes` needs. A local build since `zpz_core::file_nodes` has no standalone "DepStats
/// from a DepGraph" helper.
pub(crate) fn dep_stats_from_dep(dep: &DepGraph) -> DepStats {
    let mut fan_in = std::collections::BTreeMap::new();
    let mut fan_out = std::collections::BTreeMap::new();
    let mut all_paths = std::collections::BTreeSet::new();
    for (src, targets) in dep {
        all_paths.insert(src.clone());
        fan_out.insert(src.clone(), targets.len() as u32);
        for target in targets {
            all_paths.insert(target.clone());
            *fan_in.entry(target.clone()).or_insert(0) += 1;
        }
    }
    DepStats {
        fan_in,
        fan_out,
        all_paths,
    }
}

/// Thin delegate to `zpz_rules_graph::circular_findings`. Kept as a `crate::analyze` function (rather
/// than inlining the call at every call site) since `envelope::analyze_envelope` also imports it by
/// this name/path. `cycles` is passed in (rather than re-derived from `dep`) so this and the
/// scores/recommendations computations above share one `circular_from_dep` call.
pub(crate) fn circular_findings(cycles: &[Vec<String>]) -> Vec<Finding> {
    zpz_rules_graph::circular_findings(cycles)
}

/// Thin delegate to `zpz_rules_graph::unreachable_findings` — see `circular_findings`'s doc for why this
/// wrapper stays here rather than being inlined at its call sites.
pub(crate) fn unreachable_findings(nodes: &[FileNode], dep: &DepGraph) -> Vec<Finding> {
    zpz_rules_graph::unreachable_findings(nodes, dep)
}

/// Runs the three call-graph-BFS native rules — `zpz-rules-graph`'s `scan_unsafe_read_endpoint` /
/// `scan_non_idempotent_write` / `scan_mutating_route_no_auth` — and extends `global_findings` in place.
/// Gated behind `is_enabled` per rule id and behind having at least one reconstructed `ApiEndpoint`, so
/// a tree with no HTTP routes never pays the cost below.
///
/// ## Engine-wiring route taken
/// `FileArtifact` carries no `RawCall`s — the fused pass's contract is "parse once, project, drop the
/// AST", and `SourceSymbol`/`ImportMap` alone do not encode call sites. Rather than widen that contract,
/// this function runs a **second, uncached pass**: it re-reads every already-dispatched TypeScript
/// file's text off disk (`ts_paths`) and re-parses it with `zpz_parser_typescript::parse_calls`. This
/// never consults `zpz_cache::AnalysisCache` — a full per-file cache hit still re-reads and re-parses
/// every TS file here whenever either rule is enabled and at least one HTTP endpoint exists.
///
/// `api_endpoints` is reconstructed from the per-file `IoProvide` facts already collected (`kind ==
/// "http"`) rather than a third route-extraction pass — `IoProvide::key` is the normalized
/// `http_interface_key(method, path)` form (path params collapsed to `{}`), so a finding's displayed
/// `path` is that normalized form, not the endpoint's literal source text. This only affects display;
/// BFS correctness never depends on exact path spelling.
#[allow(clippy::too_many_arguments)]
fn run_callgraph_rules(
    root: &std::path::Path,
    config: &EngineConfig,
    io_provides: &[zpz_core::IoProvide],
    ts_paths: &HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    all_symbols: &[zpz_core::SourceSymbol],
    profile: bool,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) {
    let api_endpoints: Vec<zpz_core::ApiEndpoint> = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter_map(|p| {
            let (method, path) = p.key.split_once(' ')?;
            Some(zpz_core::ApiEndpoint {
                method: method.to_string(),
                path: path.to_string(),
                handler: p.symbol.clone().unwrap_or_default(),
                drift_ok: false,
            })
        })
        .collect();
    if api_endpoints.is_empty() {
        return;
    }

    let run_unsafe_read = is_enabled(&config.rule_config, "unsafe-read-endpoint");
    let run_non_idempotent = is_enabled(&config.rule_config, "non-idempotent-write");
    let run_mutating_no_auth = is_enabled(&config.rule_config, "mutating-route-no-auth");
    if !run_unsafe_read && !run_non_idempotent && !run_mutating_no_auth {
        return;
    }

    let mut raw_calls = Vec::new();
    let mut file_texts: HashMap<String, String> = HashMap::new();
    for rel in ts_paths {
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            raw_calls.extend(zpz_parser_typescript::parse_calls(rel, &text));
            file_texts.insert(rel.clone(), text);
        }
    }
    let imports_by_file: HashMap<String, ImportMap> = ts_import_pairs.iter().cloned().collect();
    let mut local_symbols_by_file: HashMap<String, HashSet<String>> = HashMap::new();
    for s in all_symbols {
        local_symbols_by_file
            .entry(s.file.clone())
            .or_default()
            .insert(s.name.clone());
    }
    let resolve_file_fn = |specifier: &str, from_file: &str| {
        zpz_parser_typescript::resolve_file(specifier, from_file, ts_paths)
    };
    let symbol_graph = zpz_core::callgraph::build_symbol_graph(
        &raw_calls,
        &imports_by_file,
        &local_symbols_by_file,
        &resolve_file_fn,
    );
    let write_methods: Vec<String> = zpz_rules_graph::DEFAULT_WRITE_METHODS
        .iter()
        .map(|s| s.to_string())
        .collect();

    if run_unsafe_read {
        let t0 = profile.then(Instant::now);
        let found = zpz_rules_graph::scan_unsafe_read_endpoint(
            &zpz_rules_graph::ScanUnsafeReadEndpointInput {
                api_endpoints: &api_endpoints,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                files: &file_texts,
                write_methods: &write_methods,
                orm_receiver_pattern: zpz_rules_graph::DEFAULT_ORM_RECEIVER_PATTERN,
            },
        );
        record_native_timing(rule_time, t0, "unsafe-read-endpoint", found.len());
        global_findings.extend(found);
    }
    if run_non_idempotent {
        let t0 = profile.then(Instant::now);
        let found = zpz_rules_graph::scan_non_idempotent_write(
            &zpz_rules_graph::ScanNonIdempotentWriteInput {
                api_endpoints: &api_endpoints,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                files: &file_texts,
                orm_receiver_pattern: zpz_rules_graph::DEFAULT_ORM_RECEIVER_PATTERN,
            },
        );
        record_native_timing(rule_time, t0, "non-idempotent-write", found.len());
        global_findings.extend(found);
    }
    if run_mutating_no_auth {
        // Reuses the same `symbol_graph` built above but reads `io_provides` directly rather than
        // `api_endpoints`, since the `Finding` anchors on the route's own registration `file`/`line`,
        // which `ApiEndpoint` cannot carry.
        //
        // `nest_guarded`: NestJS `@UseGuards(...)` decorator coverage, computed from the same
        // `file_texts` already read off disk — no extra file I/O. The BFS needs this side-channel
        // because a decorator application is metadata, not a call edge, so it's invisible to
        // `bfs_reachable`.
        let nest_guarded: std::collections::HashSet<(String, u32)> = file_texts
            .iter()
            .flat_map(|(rel, text)| {
                zpz_parser_typescript::extract_controller_guarded_lines(rel, text)
                    .into_iter()
                    .map(move |line| (rel.clone(), line))
            })
            .collect();
        let t0 = profile.then(Instant::now);
        let found = zpz_rules_graph::scan_mutating_route_no_auth(
            &zpz_rules_graph::ScanMutatingRouteNoAuthInput {
                io_provides,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                auth_guard_pattern: zpz_rules_graph::DEFAULT_AUTH_GUARD_PATTERN,
                nest_guarded: &nest_guarded,
            },
        );
        record_native_timing(rule_time, t0, "mutating-route-no-auth", found.len());
        global_findings.extend(found);
    }
}

/// Whole-corpus Java Spring HTTP-provides pass — wires `zpz_parser_java::extract_http_provides_project`
/// (see that module's doc for the two per-file-invisible facts it resolves: CE-split `extends`-chain
/// gating, and constant/constant-concatenation class-level `@RequestMapping` prefixes) into `assemble`.
/// Runs once per `analyze_tree` call, over EVERY non-degraded java-dispatched file (`java_rels`),
/// reading each file's text fresh off disk — the fused per-file pass drops each file's text after
/// projecting its own slice, and folding a whole-corpus-dependent result into the per-file cache would
/// let an edit to one file (e.g. a prefix-constants-only file with no routes of its own) leave every
/// OTHER already-cached java file's provides silently stale. Recomputed in full on every call — never
/// consults `zpz_cache::AnalysisCache`.
///
/// **Merge semantics**: `io_provides` already carries the fused per-file pass's own java `http` provides
/// — same-file controllers with a literal (or absent) class-level `@RequestMapping`. The project pass
/// finds a superset of that, with one known exception: a controller whose simple class name is
/// duplicated across the corpus is skipped by the project pass's ambiguous-class guard even when its
/// prefix is literal, so its per-file provides are deleted by this replacement without a project-side
/// substitute (route loss). Accepted: duplicate controller class names are rare, and key-based dedupe
/// instead would leave a latent trap where the two passes silently disagree on one fact and both
/// entries survive. So this REPLACES the per-file java `http` provides wholesale with the project
/// pass's own output, for every file in `java_rels`: one source of truth.
fn run_java_provides_project_pass(
    root: &std::path::Path,
    java_rels: &[String],
    io_provides: &mut Vec<IoProvide>,
) {
    let java_set: HashSet<&str> = java_rels.iter().map(String::as_str).collect();
    let mut files: Vec<(String, String)> = Vec::with_capacity(java_rels.len());
    for rel in java_rels {
        // Unreadable (deleted/permission race since the fused pass's own read) — same "treat as absent
        // rather than fail the whole analysis" convention `dead_export_findings` documents for its own
        // disk re-read.
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            files.push((rel.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    if files.is_empty() {
        return;
    }
    let report = zpz_parser_java::extract_http_provides_project(&files);
    io_provides.retain(|p| !(p.kind == "http" && java_set.contains(p.file.as_str())));
    io_provides.extend(report.provides);
}

/// Runs the three schema x usage JOIN native rules (`soft-delete-bypass` / `orderby-unindexed` /
/// `enum-string-drift` — `zpz_rules_schema::join`'s module doc) — a whole-tree pass over every
/// non-degraded Prisma file (`prisma_rels`, same eligibility as `schema-usage`) plus a fresh
/// `scan_query_call_sites` walk of the BE source tree, gated per-id via `is_enabled` and timed via
/// `record_native_timing`, the same shape every other whole-tree native analysis in `assemble` uses.
///
/// `enum-string-drift` also collects `SchemaEnum`s (via `zpz_parser_prisma::parse_schema_enums`,
/// alongside the per-file `parse_schema` call for models) over the same `prisma_rels`, so
/// `enum_string_drift_issues` has both model and enum substrate to join call-site literals against.
///
/// All three rules need evidence spanning the whole BE source tree (every query call site for a model,
/// not just one file), so this is recomputed in full on every `assemble` call and never enters the
/// per-file findings cache.
fn run_schema_join_rules(
    root: &std::path::Path,
    prisma_rels: &[String],
    config: &EngineConfig,
    profile: bool,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) {
    if prisma_rels.is_empty() {
        return;
    }
    if !is_enabled(&config.rule_config, "soft-delete-bypass")
        && !is_enabled(&config.rule_config, "orderby-unindexed")
        && !is_enabled(&config.rule_config, "enum-string-drift")
    {
        return;
    }

    let mut models: Vec<zpz_core::SchemaModel> = Vec::new();
    let mut enums: Vec<zpz_core::SchemaEnum> = Vec::new();
    for rel in prisma_rels {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        models.extend(zpz_parser_prisma::parse_schema(&text, Some(rel), None));
        enums.extend(zpz_parser_prisma::parse_schema_enums(&text));
    }
    if models.is_empty() {
        return;
    }

    let sites = zpz_rules_schema::scan_query_call_sites(
        root,
        zpz_parser_prisma::DEFAULT_PRISMA_CLIENT_GETTER_FN,
    );

    run_join_rule(
        "soft-delete-bypass",
        &config.rule_config,
        profile,
        &models,
        &sites,
        zpz_rules_schema::soft_delete_bypass_issues,
        rule_time,
        global_findings,
    );
    run_join_rule(
        "orderby-unindexed",
        &config.rule_config,
        profile,
        &models,
        &sites,
        zpz_rules_schema::orderby_unindexed_issues,
        rule_time,
        global_findings,
    );
    run_join_rule(
        "enum-string-drift",
        &config.rule_config,
        profile,
        &models,
        &sites,
        |m, s| zpz_rules_schema::enum_string_drift_issues(m, &enums, s),
        rule_time,
        global_findings,
    );
}

/// Runs one schema x usage JOIN rule (`rule_fn`) under the `id` gate, appending its findings to
/// `global_findings` and timing the call. `rule_fn` is generic (not a bare `fn` pointer) so
/// `enum-string-drift`'s call site can close over its extra `enums` argument via a closure while the
/// other two rules' plain `fn` items keep coercing in unchanged.
#[allow(clippy::too_many_arguments)]
fn run_join_rule<F>(
    id: &str,
    rule_config: &zpz_core::RuleConfig,
    profile: bool,
    models: &[zpz_core::SchemaModel],
    sites: &[zpz_rules_schema::QueryCallSite],
    rule_fn: F,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) where
    F: Fn(
        &[zpz_core::SchemaModel],
        &[zpz_rules_schema::QueryCallSite],
    ) -> Vec<zpz_rules_schema::JoinIssue>,
{
    if !is_enabled(rule_config, id) {
        return;
    }
    let t0 = profile.then(Instant::now);
    let issues = rule_fn(models, sites);
    let found: Vec<Finding> = issues.iter().map(join_issue_to_finding).collect();
    record_native_timing(rule_time, t0, id, found.len());
    global_findings.extend(found);
}

/// One `JoinIssue` -> one `Finding`. Unlike `schema_issue_to_finding` (`pipeline.rs`), no
/// `zpz_parser_prisma::model_decl_line` lookup is needed: `JoinIssue` already carries the exact BE
/// call-site `file`/`line` it fired at. `rule_id` is the bare id, not `"schema/{id}"` — each of these
/// three is a whole individually-gated toggle unit, matching `duplicate-route`'s convention rather than
/// `schema-usage`'s pack-namespace-prefixed sub-rule ids.
fn join_issue_to_finding(issue: &zpz_rules_schema::JoinIssue) -> Finding {
    Finding {
        rule_id: issue.rule.clone(),
        severity: issue.severity,
        file: issue.file.clone(),
        line: issue.line,
        message: zpz_rules_schema::join_issue_message(issue),
        data: serde_json::to_value(issue).ok(),
    }
}

/// Thin delegate to `zpz_rules_graph::dead_candidate_findings` — see `circular_findings`'s doc. `extra_entries`
/// forwards straight through (package.json-referenced entry files).
pub(crate) fn dead_candidate_findings(
    nodes: &[FileNode],
    dep: &DepGraph,
    extra_entries: &HashSet<String>,
) -> Vec<Finding> {
    zpz_rules_graph::dead_candidate_findings(nodes, dep, extra_entries)
}

/// Join per-file wrapper CALL fragments against wrapper DEFINITION fragments and emit an `http`
/// `IoConsume` at each resolvable CALL site — the consume-side twin of the provide composers: the
/// wrapper's own body only ever shows egress a non-literal sink (`axios.request(options)`), so
/// without this join a project-local request-wrapper family is invisible and every consume anchor
/// points at wrapper internals instead of the code a reader would edit.
///
/// Resolution: a call's `callee` finds its def in the SAME file first (local wrapper), else via
/// `resolve(specifier, from_file)` → that file's def of the same name (the same workspace-aware
/// resolver the provide composers use). Method = the def's `fixed_method` or the call's
/// `method_param`-indexed arg (must be a literal GET/POST/PUT/PATCH/DELETE — anything else skips
/// the call, never guesses); path = the `path_param`-indexed arg (must start with `/`). Emitted
/// consumes are fully keyed (no late resolution) and deduped/sorted deterministically.
pub(crate) fn resolve_wrapper_consumes(
    def_pairs: Vec<(String, Vec<zpz_core::WrapperDefFragment>)>,
    call_pairs: Vec<(String, Vec<zpz_core::WrapperCallFragment>)>,
    resolve: impl Fn(&str, &str) -> Option<String>,
    io_consumes: &mut Vec<IoConsume>,
) {
    const VERBS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

    let mut defs: HashMap<(String, String), &zpz_core::WrapperDefFragment> = HashMap::new();
    for (file, frags) in &def_pairs {
        for def in frags {
            defs.insert((file.clone(), def.name.clone()), def);
        }
    }

    let mut call_pairs = call_pairs;
    call_pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out: Vec<IoConsume> = Vec::new();
    for (file, calls) in &call_pairs {
        for call in calls {
            let def_file = match &call.specifier {
                None => Some(file.clone()),
                Some(spec) => resolve(spec, file),
            };
            let def = def_file.and_then(|f| defs.get(&(f, call.callee.clone())).copied());
            let Some(def) = def else { continue };
            let method = match (&def.fixed_method, def.method_param) {
                (Some(m), _) => Some(m.clone()),
                // Any-case verb literal accepted and uppercased — the same tolerance
                // `egress::method_from_options` applies (its own tests use `method: "delete"`).
                (None, Some(idx)) => call
                    .args
                    .get(idx as usize)
                    .and_then(|a| a.clone())
                    .map(|m| m.to_ascii_uppercase())
                    .filter(|m| VERBS.contains(&m.as_str())),
                (None, None) => None,
            };
            let Some(method) = method else { continue };
            let path = call
                .args
                .get(def.path_param as usize)
                .and_then(|a| a.clone())
                .filter(|p| p.starts_with('/'));
            let Some(path) = path else { continue };
            out.push(IoConsume {
                kind: "http".to_string(),
                key: Some(zpz_core::http_interface_key(&method, &path)),
                file: file.clone(),
                line: call.line,
                raw: None,
                method: None,
            });
        }
    }

    out.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out.dedup_by(|a, b| a.key == b.key && a.file == b.file && a.line == b.line);
    io_consumes.extend(out);
}

/// Compose whole-tree `http` PROVIDEs from per-file router-mount fragments
/// (`zpz_parser_typescript::router_mounts` — Hono-style chained builders and cross-file
/// `.route(prefix, subRouter)` mounts). The provide-side twin of [`compose_trpc_provides`]: same
/// root-exclusion conservatism, same import-resolver closure, same dedup/sort discipline; composition
/// joins URL path prefixes instead of dotted procedure paths.
///
/// **Roots**: a fragment is a DFS root only when nothing mounts it — neither by resolved edge nor by
/// NAME (some `Mount.ident` anywhere equals its name, even unresolved): a mounted-but-unresolvable
/// sub-router must not surface its entries with a truncated (missing prefix) URL — under-reporting is
/// honest, mis-keying is not.
///
/// **Mount resolution**: same-file fragment named `ident` first; else `resolve(specifier)` →
/// target file, preferring the fragment named `ident` and falling back to the file's SOLE fragment
/// (covers `export default route` re-imported under an arbitrary local alias — the common
/// one-router-per-file layout). Ambiguous (multi-fragment, no name match) or unresolvable mounts
/// skip that subtree.
///
/// Provide anchors: `file`/`line` of the VERB registration (the leaf file, not the mount site),
/// `symbol` = handler name — the place a reader would edit the route.
pub(crate) fn compose_router_mount_provides(
    fragments: Vec<(String, Vec<zpz_core::RouterMountFragment>)>,
    resolve: impl Fn(&str, &str) -> Option<String>,
) -> Vec<IoProvide> {
    use zpz_core::{RouterMountEntry, RouterMountFragment};

    let mut fragments = fragments;
    fragments.sort_by(|a, b| a.0.cmp(&b.0));

    // (file, fragment) node list + per-file index; nodes keep per-file source order.
    let mut nodes: Vec<(&str, &RouterMountFragment)> = Vec::new();
    let mut by_file: HashMap<&str, Vec<usize>> = HashMap::new();
    for (file, frags) in &fragments {
        for frag in frags {
            by_file.entry(file.as_str()).or_default().push(nodes.len());
            nodes.push((file.as_str(), frag));
        }
    }

    let find_child = |from_file: &str, ident: &str, specifier: Option<&str>| -> Option<usize> {
        let candidates_in = |file: &str| -> Option<usize> {
            let idxs = by_file.get(file)?;
            if let Some(&idx) = idxs.iter().find(|&&i| nodes[i].1.name == ident) {
                return Some(idx);
            }
            if idxs.len() == 1 {
                return Some(idxs[0]);
            }
            None
        };
        match specifier {
            None => candidates_in(from_file),
            Some(spec) => {
                let target = resolve(spec, from_file)?;
                candidates_in(&target)
            }
        }
    };

    // Root exclusion: mounted by name anywhere (unresolved-conservative) OR by resolved edge.
    let mut mounted_names: HashSet<&str> = HashSet::new();
    let mut mounted_nodes: HashSet<usize> = HashSet::new();
    for (file, frag) in &nodes {
        for entry in &frag.entries {
            if let RouterMountEntry::Mount {
                ident, specifier, ..
            } = entry
            {
                mounted_names.insert(ident.as_str());
                if let Some(child) = find_child(file, ident, specifier.as_deref()) {
                    mounted_nodes.insert(child);
                }
            }
        }
    }

    fn join_prefix(prefix: &str, seg: &str) -> String {
        if seg == "/" || seg.is_empty() {
            return prefix.to_string();
        }
        let base = prefix.trim_end_matches('/');
        if seg.starts_with('/') {
            format!("{base}{seg}")
        } else {
            format!("{base}/{seg}")
        }
    }

    /// `(from_file, ident, specifier)` → node index of the mounted child fragment, if resolvable.
    type FindChild<'a> = dyn Fn(&str, &str, Option<&str>) -> Option<usize> + 'a;

    #[allow(clippy::too_many_arguments)]
    fn walk(
        idx: usize,
        prefix: &str,
        nodes: &[(&str, &zpz_core::RouterMountFragment)],
        find_child: &FindChild,
        ancestry: &mut Vec<usize>,
        out: &mut Vec<IoProvide>,
    ) {
        if ancestry.contains(&idx) {
            return; // cycle guard — mirrors compose_trpc_provides' ancestry stack
        }
        ancestry.push(idx);
        let (file, frag) = nodes[idx];
        for entry in &frag.entries {
            match entry {
                zpz_core::RouterMountEntry::Verb {
                    method,
                    path,
                    handler,
                    line,
                } => {
                    let full = join_prefix(prefix, path);
                    out.push(IoProvide {
                        kind: "http".to_string(),
                        key: zpz_core::http_interface_key(method, &full),
                        file: file.to_string(),
                        line: *line,
                        symbol: handler.clone(),
                    });
                }
                zpz_core::RouterMountEntry::Mount {
                    prefix: mount_prefix,
                    ident,
                    specifier,
                } => {
                    if let Some(child) = find_child(file, ident, specifier.as_deref()) {
                        walk(
                            child,
                            &join_prefix(prefix, mount_prefix),
                            nodes,
                            find_child,
                            ancestry,
                            out,
                        );
                    }
                }
            }
        }
        ancestry.pop();
    }

    let mut out: Vec<IoProvide> = Vec::new();
    for idx in 0..nodes.len() {
        if mounted_nodes.contains(&idx) || mounted_names.contains(nodes[idx].1.name.as_str()) {
            continue;
        }
        let mut ancestry = Vec::new();
        walk(idx, "", &nodes, &find_child, &mut ancestry, &mut out);
    }

    out.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out.dedup_by(|a, b| a.key == b.key && a.file == b.file && a.line == b.line);
    out
}

#[cfg(test)]
mod late_resolve_tests {
    use super::*;

    fn unresolved(raw: &str, method: &str) -> IoConsume {
        IoConsume {
            kind: "http".to_string(),
            key: None,
            file: "src/caller.ts".to_string(),
            line: 1,
            raw: Some(raw.to_string()),
            method: Some(method.to_string()),
        }
    }

    fn consts(entries: &[(&str, &str)]) -> Vec<(String, HashMap<String, String>)> {
        let fragment: HashMap<String, String> = entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        vec![("src/consts.ts".to_string(), fragment)]
    }

    #[test]
    fn slash_value_resolves_to_a_normalized_internal_key() {
        let mut consumes = vec![unresolved("Api.user", "GET")];
        late_resolve_cross_file_consumes(consts(&[("Api.user", "/api/user/")]), &mut consumes);
        assert_eq!(consumes[0].key.as_deref(), Some("GET /api/user"));
        assert!(consumes[0].raw.is_some()); // provenance retained
    }

    #[test]
    fn absolute_url_value_keeps_the_verbatim_external_key() {
        let mut consumes = vec![unresolved("Api.vendor", "POST")];
        late_resolve_cross_file_consumes(
            consts(&[("Api.vendor", "https://vendor.com/x")]),
            &mut consumes,
        );
        // Verbatim -- `link_cross_layer_io`'s `"://"` gate must still see the host.
        assert_eq!(
            consumes[0].key.as_deref(),
            Some("POST https://vendor.com/x")
        );
    }

    #[test]
    fn bare_fragment_value_stays_unresolved() {
        let mut consumes = vec![unresolved("Api.frag", "GET")];
        late_resolve_cross_file_consumes(
            consts(&[("Api.frag", "authen/getUserInfo")]),
            &mut consumes,
        );
        assert_eq!(consumes[0].key, None);
    }
}

#[cfg(test)]
mod wrapper_consume_tests {
    //! Coverage for `resolve_wrapper_consumes`: cross-file join via specifier, same-file local
    //! wrapper, fixed-method wrappers, the never-guess skips (non-verb method arg, non-`/` path,
    //! unresolvable specifier), and determinism.
    use super::*;
    use zpz_core::{WrapperCallFragment, WrapperDefFragment};

    fn def(
        name: &str,
        method_param: Option<u32>,
        path_param: u32,
        fixed: Option<&str>,
    ) -> WrapperDefFragment {
        WrapperDefFragment {
            name: name.to_string(),
            method_param,
            path_param,
            fixed_method: fixed.map(str::to_string),
        }
    }

    fn call(
        callee: &str,
        specifier: Option<&str>,
        args: Vec<Option<&str>>,
        line: u32,
    ) -> WrapperCallFragment {
        WrapperCallFragment {
            callee: callee.to_string(),
            specifier: specifier.map(str::to_string),
            args: args.into_iter().map(|a| a.map(str::to_string)).collect(),
            line,
        }
    }

    fn resolver<'a>(
        map: &'a [(&'a str, &'a str, &'a str)],
    ) -> impl Fn(&str, &str) -> Option<String> + 'a {
        move |spec: &str, from: &str| {
            map.iter()
                .find(|(s, f, _)| *s == spec && *f == from)
                .map(|(_, _, t)| t.to_string())
        }
    }

    #[test]
    fn imported_wrapper_call_becomes_a_keyed_consume_at_the_call_site() {
        let defs = vec![(
            "utils/api.ts".to_string(),
            vec![def("makeRestApiRequest", Some(1), 2, None)],
        )];
        let calls = vec![(
            "src/api/workflows.ts".to_string(),
            vec![
                call(
                    "makeRestApiRequest",
                    Some("@/utils/api"),
                    vec![None, Some("GET"), Some("/workflows/new")],
                    12,
                ),
                call(
                    "makeRestApiRequest",
                    Some("@/utils/api"),
                    vec![None, Some("POST"), Some("/workflows/{}/activate"), None],
                    30,
                ),
            ],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(
            defs,
            calls,
            resolver(&[("@/utils/api", "src/api/workflows.ts", "utils/api.ts")]),
            &mut consumes,
        );
        let keys: Vec<&str> = consumes.iter().flat_map(|c| c.key.as_deref()).collect();
        assert_eq!(
            keys,
            vec!["GET /workflows/new", "POST /workflows/{}/activate"]
        );
        assert_eq!(consumes[0].file, "src/api/workflows.ts");
        assert_eq!(consumes[0].line, 12);
    }

    #[test]
    fn fixed_method_wrapper_and_same_file_local_call() {
        let defs = vec![(
            "src/stream.ts".to_string(),
            vec![def("streamRequest", None, 1, Some("POST"))],
        )];
        let calls = vec![(
            "src/stream.ts".to_string(),
            vec![call(
                "streamRequest",
                None,
                vec![None, Some("/ai/chat")],
                40,
            )],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(defs, calls, |_, _| None, &mut consumes);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key.as_deref(), Some("POST /ai/chat"));
    }

    #[test]
    fn never_guesses_on_non_verb_non_path_or_unresolvable() {
        let defs = vec![(
            "utils/api.ts".to_string(),
            vec![def("makeRestApiRequest", Some(1), 2, None)],
        )];
        let calls = vec![(
            "src/a.ts".to_string(),
            vec![
                // method arg is a variable, not a literal verb
                call(
                    "makeRestApiRequest",
                    Some("./u"),
                    vec![None, None, Some("/x")],
                    1,
                ),
                // path arg does not start with '/'
                call(
                    "makeRestApiRequest",
                    Some("./u"),
                    vec![None, Some("GET"), Some("x")],
                    2,
                ),
                // unresolvable specifier
                call(
                    "makeRestApiRequest",
                    Some("./nowhere"),
                    vec![None, Some("GET"), Some("/x")],
                    3,
                ),
            ],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(
            defs,
            calls,
            resolver(&[("./u", "src/a.ts", "utils/api.ts")]),
            &mut consumes,
        );
        assert!(consumes.is_empty());
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let defs = vec![("u.ts".to_string(), vec![def("w", Some(0), 1, None)])];
        let build = |rev: bool| {
            let mut v = vec![
                (
                    "a.ts".to_string(),
                    vec![call("w", Some("./u"), vec![Some("GET"), Some("/a")], 1)],
                ),
                (
                    "b.ts".to_string(),
                    vec![call("w", Some("./u"), vec![Some("GET"), Some("/b")], 1)],
                ),
            ];
            if rev {
                v.reverse();
            }
            v
        };
        let run = |calls| {
            let mut out = Vec::new();
            resolve_wrapper_consumes(
                defs.clone(),
                calls,
                resolver(&[("./u", "a.ts", "u.ts"), ("./u", "b.ts", "u.ts")]),
                &mut out,
            );
            out.into_iter()
                .map(|c| (c.key, c.file, c.line))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(build(false)), run(build(true)));
    }
}

#[cfg(test)]
mod router_mount_compose_tests {
    //! Coverage for `compose_router_mount_provides`: same-file mount join, a 3-hop mount chain across
    //! files, `/`-prefix passthrough, root exclusion (mounted child never emitted unprefixed;
    //! unresolvable-but-named child skipped wholesale), sole-fragment fallback for default-import
    //! aliases, cycle guard, determinism.
    use super::*;
    use zpz_core::{RouterMountEntry, RouterMountFragment};

    fn verb(method: &str, path: &str, handler: &str, line: u32) -> RouterMountEntry {
        RouterMountEntry::Verb {
            method: method.to_string(),
            path: path.to_string(),
            handler: Some(handler.to_string()),
            line,
        }
    }

    fn mount(prefix: &str, ident: &str, specifier: Option<&str>) -> RouterMountEntry {
        RouterMountEntry::Mount {
            prefix: prefix.to_string(),
            ident: ident.to_string(),
            specifier: specifier.map(str::to_string),
        }
    }

    fn frag(name: &str, entries: Vec<RouterMountEntry>) -> RouterMountFragment {
        RouterMountFragment {
            name: name.to_string(),
            entries,
        }
    }

    fn no_resolver() -> impl Fn(&str, &str) -> Option<String> {
        |_: &str, _: &str| None
    }

    /// Maps (specifier, from_file) pairs to target rel paths.
    fn resolver<'a>(
        map: &'a [(&'a str, &'a str, &'a str)],
    ) -> impl Fn(&str, &str) -> Option<String> + 'a {
        move |spec: &str, from: &str| {
            map.iter()
                .find(|(s, f, _)| *s == spec && *f == from)
                .map(|(_, _, t)| t.to_string())
        }
    }

    #[test]
    fn same_file_mount_joins_prefix() {
        let out = compose_router_mount_provides(
            vec![(
                "src/app.ts".to_string(),
                vec![
                    frag(
                        "app",
                        vec![
                            verb("GET", "/health", "h", 2),
                            mount("/admin", "adminRouter", None),
                        ],
                    ),
                    frag("adminRouter", vec![verb("POST", "/users", "createUser", 9)]),
                ],
            )],
            no_resolver(),
        );
        let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, vec!["GET /health", "POST /admin/users"]);
        assert_eq!(out[1].file, "src/app.ts");
        assert_eq!(out[1].line, 9);
        assert_eq!(out[1].symbol.as_deref(), Some("createUser"));
    }

    #[test]
    fn three_hop_mount_chain_composes_full_url() {
        // server/router.ts mounts auth at /api/auth; auth/index.ts mounts twoFactorRoute at
        // /two-factor (plus an inline verb and a "/"-passthrough mount); the leaf file registers
        // POST /setup. Expected: /api/auth/two-factor/setup with the LEAF file:line anchor.
        let fragments = vec![
            (
                "auth/index.ts".to_string(),
                vec![frag(
                    "auth",
                    vec![
                        verb("GET", "/csrf", "csrfHandler", 21),
                        mount("/", "sessionRoute", Some("./routes/session")),
                        mount("/two-factor", "twoFactorRoute", Some("./routes/two-factor")),
                    ],
                )],
            ),
            (
                "auth/routes/session.ts".to_string(),
                vec![frag("sessionRoute", vec![verb("GET", "/session", "s", 5)])],
            ),
            (
                "auth/routes/two-factor.ts".to_string(),
                vec![frag(
                    "twoFactorRoute",
                    vec![verb("POST", "/setup", "setup", 20)],
                )],
            ),
            (
                "server/router.ts".to_string(),
                vec![frag(
                    "app",
                    vec![mount("/api/auth", "auth", Some("@example/auth-server"))],
                )],
            ),
        ];
        let out = compose_router_mount_provides(
            fragments,
            resolver(&[
                (
                    "./routes/session",
                    "auth/index.ts",
                    "auth/routes/session.ts",
                ),
                (
                    "./routes/two-factor",
                    "auth/index.ts",
                    "auth/routes/two-factor.ts",
                ),
                ("@example/auth-server", "server/router.ts", "auth/index.ts"),
            ]),
        );
        let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "GET /api/auth/csrf",
                "GET /api/auth/session",
                "POST /api/auth/two-factor/setup",
            ]
        );
        assert_eq!(out[2].file, "auth/routes/two-factor.ts");
        assert_eq!(out[2].line, 20);
    }

    #[test]
    fn mounted_child_is_never_emitted_unprefixed_even_when_unresolvable() {
        // `admin` is mounted by name from a file the resolver cannot link — the child fragment
        // must NOT surface `/users` with the missing `/admin` prefix (conservative root
        // exclusion, mirroring compose_trpc_provides).
        let fragments = vec![
            (
                "src/app.ts".to_string(),
                vec![frag(
                    "app",
                    vec![mount("/admin", "admin", Some("./nowhere"))],
                )],
            ),
            (
                "src/admin.ts".to_string(),
                vec![frag("admin", vec![verb("GET", "/users", "h", 3)])],
            ),
        ];
        let out = compose_router_mount_provides(fragments, no_resolver());
        assert!(out.is_empty());
    }

    #[test]
    fn sole_fragment_fallback_covers_default_import_alias() {
        // `export default route` re-imported as `pdfRoute` — no name match in the target file,
        // but it holds exactly one fragment, so the mount resolves to it.
        let fragments = vec![
            (
                "server/files.ts".to_string(),
                vec![frag(
                    "filesRoute",
                    vec![mount("/", "pdfRoute", Some("./routes/pdf"))],
                )],
            ),
            (
                "server/routes/pdf.ts".to_string(),
                vec![frag(
                    "route",
                    vec![verb("GET", "/envelope/:id/item.pdf", "h", 4)],
                )],
            ),
        ];
        let out = compose_router_mount_provides(
            fragments,
            resolver(&[("./routes/pdf", "server/files.ts", "server/routes/pdf.ts")]),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "GET /envelope/{}/item.pdf");
    }

    #[test]
    fn mount_cycle_is_guarded() {
        let fragments = vec![(
            "src/a.ts".to_string(),
            vec![
                frag("a", vec![verb("GET", "/x", "h", 1), mount("/b", "b", None)]),
                frag("b", vec![mount("/a", "a", None)]),
            ],
        )];
        let out = compose_router_mount_provides(fragments, no_resolver());
        // `a` and `b` mount each other, so neither is a root — conservative empty output rather
        // than an infinite walk or a truncated-prefix guess.
        assert!(out.is_empty());
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let build = |rev: bool| {
            let mut v = vec![
                (
                    "src/app.ts".to_string(),
                    vec![frag(
                        "app",
                        vec![mount("/api", "sub", None), verb("GET", "/", "root", 1)],
                    )],
                ),
                (
                    "src/app.ts".to_string(),
                    vec![frag("sub", vec![verb("POST", "/items", "create", 8)])],
                ),
            ];
            if rev {
                v.reverse();
            }
            v
        };
        let a = compose_router_mount_provides(build(false), no_resolver());
        let b = compose_router_mount_provides(build(true), no_resolver());
        let view = |v: &[IoProvide]| -> Vec<(String, String, u32)> {
            v.iter()
                .map(|p| (p.key.clone(), p.file.clone(), p.line))
                .collect()
        };
        assert_eq!(view(&a), view(&b));
    }
}

#[cfg(test)]
mod trpc_compose_tests {
    //! Coverage for `compose_trpc_provides`: inline nested + leaf composition, cross-file `Ref` via
    //! specifier, same-file `Ref` by name, `mergeRouters` empty-key splice, an unresolvable `Ref` skipped
    //! (sibling entries survive), a self-referencing cycle guarded against infinite recursion, and
    //! determinism under input-order reshuffling.
    use super::*;
    use zpz_core::{TrpcRouterEntry, TrpcRouterFragment};

    /// A resolver that only ever answers the exact `(specifier, from_file)` pairs listed — anything else is
    /// `None`, mirroring how a real unresolvable/external specifier behaves.
    fn resolver(
        table: &'static [(&'static str, &'static str, &'static str)],
    ) -> impl Fn(&str, &str) -> Option<String> {
        move |specifier, from_file| {
            table
                .iter()
                .find(|(s, f, _)| *s == specifier && *f == from_file)
                .map(|(_, _, target)| target.to_string())
        }
    }

    fn no_resolver() -> impl Fn(&str, &str) -> Option<String> {
        |_, _| None
    }

    fn frag(name: &str, entries: Vec<TrpcRouterEntry>) -> TrpcRouterFragment {
        TrpcRouterFragment {
            name: name.to_string(),
            entries,
        }
    }

    fn keys(out: &[IoProvide]) -> Vec<(String, String, u32)> {
        out.iter()
            .map(|p| (p.key.clone(), p.file.clone(), p.line))
            .collect()
    }

    #[test]
    fn root_with_inline_nested_and_leaf() {
        let fragments = vec![(
            "a.ts".to_string(),
            vec![frag(
                "appRouter",
                vec![
                    TrpcRouterEntry::Nested {
                        key: "greeting".into(),
                        entries: vec![TrpcRouterEntry::Leaf {
                            key: "hello".into(),
                            verb: "QUERY".into(),
                            line: 2,
                        }],
                    },
                    TrpcRouterEntry::Leaf {
                        key: "ping".into(),
                        verb: "QUERY".into(),
                        line: 5,
                    },
                ],
            )],
        )];
        let out = compose_trpc_provides(fragments, no_resolver());
        assert_eq!(
            keys(&out),
            vec![
                ("QUERY greeting.hello".to_string(), "a.ts".to_string(), 2),
                ("QUERY ping".to_string(), "a.ts".to_string(), 5),
            ]
        );
    }

    #[test]
    fn ref_via_specifier_resolves_to_another_files_fragment() {
        let fragments = vec![
            (
                "trpc.ts".to_string(),
                vec![frag(
                    "appRouter",
                    vec![TrpcRouterEntry::Ref {
                        key: "viewer".into(),
                        ident: "viewerRouter".into(),
                        specifier: Some("./viewer".into()),
                    }],
                )],
            ),
            (
                "viewer.ts".to_string(),
                vec![frag(
                    "viewerRouter",
                    vec![TrpcRouterEntry::Leaf {
                        key: "me".into(),
                        verb: "QUERY".into(),
                        line: 1,
                    }],
                )],
            ),
        ];
        let out =
            compose_trpc_provides(fragments, resolver(&[("./viewer", "trpc.ts", "viewer.ts")]));
        assert_eq!(
            keys(&out),
            vec![("QUERY viewer.me".to_string(), "viewer.ts".to_string(), 1)]
        );
    }

    #[test]
    fn same_file_ref_by_name_has_no_specifier() {
        let fragments = vec![(
            "r.ts".to_string(),
            vec![
                frag(
                    "outer",
                    vec![TrpcRouterEntry::Ref {
                        key: "nested".into(),
                        ident: "inner".into(),
                        specifier: None,
                    }],
                ),
                frag(
                    "inner",
                    vec![TrpcRouterEntry::Leaf {
                        key: "x".into(),
                        verb: "QUERY".into(),
                        line: 3,
                    }],
                ),
            ],
        )];
        let out = compose_trpc_provides(fragments, no_resolver());
        assert_eq!(
            keys(&out),
            vec![("QUERY nested.x".to_string(), "r.ts".to_string(), 3)]
        );
    }

    #[test]
    fn merge_routers_empty_key_splices_at_the_current_level() {
        let fragments = vec![
            (
                "r.ts".to_string(),
                vec![frag(
                    "combined",
                    vec![
                        TrpcRouterEntry::Ref {
                            key: String::new(),
                            ident: "aRouter".into(),
                            specifier: Some("./a".into()),
                        },
                        TrpcRouterEntry::Ref {
                            key: String::new(),
                            ident: "bRouter".into(),
                            specifier: Some("./b".into()),
                        },
                    ],
                )],
            ),
            (
                "a.ts".to_string(),
                vec![frag(
                    "aRouter",
                    vec![TrpcRouterEntry::Leaf {
                        key: "x".into(),
                        verb: "QUERY".into(),
                        line: 1,
                    }],
                )],
            ),
            (
                "b.ts".to_string(),
                vec![frag(
                    "bRouter",
                    vec![TrpcRouterEntry::Leaf {
                        key: "y".into(),
                        verb: "MUTATION".into(),
                        line: 2,
                    }],
                )],
            ),
        ];
        let out = compose_trpc_provides(
            fragments,
            resolver(&[("./a", "r.ts", "a.ts"), ("./b", "r.ts", "b.ts")]),
        );
        assert_eq!(
            keys(&out),
            vec![
                ("MUTATION y".to_string(), "b.ts".to_string(), 2),
                ("QUERY x".to_string(), "a.ts".to_string(), 1),
            ]
        );
    }

    #[test]
    fn unresolvable_ref_is_skipped_sibling_leaf_survives() {
        let fragments = vec![(
            "a.ts".to_string(),
            vec![frag(
                "appRouter",
                vec![
                    TrpcRouterEntry::Ref {
                        key: "missing".into(),
                        ident: "ghost".into(),
                        specifier: Some("./ghost".into()),
                    },
                    TrpcRouterEntry::Leaf {
                        key: "ok".into(),
                        verb: "QUERY".into(),
                        line: 1,
                    },
                ],
            )],
        )];
        // resolver answers nothing -> `./ghost` never resolves; `ghost` also names no known fragment even
        // if it did.
        let out = compose_trpc_provides(fragments, no_resolver());
        assert_eq!(
            keys(&out),
            vec![("QUERY ok".to_string(), "a.ts".to_string(), 1)]
        );
    }

    #[test]
    fn self_referencing_cycle_is_guarded_without_infinite_recursion() {
        let fragments = vec![(
            "app.ts".to_string(),
            vec![
                frag(
                    "app",
                    vec![TrpcRouterEntry::Ref {
                        key: "a".into(),
                        ident: "a".into(),
                        specifier: None,
                    }],
                ),
                frag(
                    "a",
                    vec![
                        TrpcRouterEntry::Leaf {
                            key: "x".into(),
                            verb: "QUERY".into(),
                            line: 5,
                        },
                        // Cycles back to itself — must be skipped, not re-composed.
                        TrpcRouterEntry::Ref {
                            key: "loop".into(),
                            ident: "a".into(),
                            specifier: None,
                        },
                    ],
                ),
            ],
        )];
        let out = compose_trpc_provides(fragments, no_resolver());
        assert_eq!(
            keys(&out),
            vec![("QUERY a.x".to_string(), "app.ts".to_string(), 5)]
        );
    }

    #[test]
    fn composition_is_deterministic_under_input_order_reshuffling() {
        let build = |reversed: bool| {
            let mut fragments = vec![
                (
                    "trpc.ts".to_string(),
                    vec![frag(
                        "appRouter",
                        vec![TrpcRouterEntry::Ref {
                            key: "viewer".into(),
                            ident: "viewerRouter".into(),
                            specifier: Some("./viewer".into()),
                        }],
                    )],
                ),
                (
                    "viewer.ts".to_string(),
                    vec![frag(
                        "viewerRouter",
                        vec![TrpcRouterEntry::Leaf {
                            key: "me".into(),
                            verb: "QUERY".into(),
                            line: 1,
                        }],
                    )],
                ),
            ];
            if reversed {
                fragments.reverse();
            }
            fragments
        };
        let resolve = || resolver(&[("./viewer", "trpc.ts", "viewer.ts")]);
        let out1 = compose_trpc_provides(build(false), resolve());
        let out2 = compose_trpc_provides(build(true), resolve());
        assert_eq!(keys(&out1), keys(&out2));
    }
}
