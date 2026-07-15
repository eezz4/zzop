//! Assembly + whole-graph pass — runs after the fused per-file pass (`pipeline::run_file_pass`) has
//! already dropped every parser's AST. Operates on plain `zzop_core` data: `FileArtifact`s -> one
//! tree-wide `CommonIr` -> whole-graph native analyses (circular / unreachable / dead-candidates) ->
//! `merge_findings` with the per-file DSL findings collected during the fused pass.
//!
//! Also runs the optional git-history-dependent analyses: when `EngineConfig::git` is `Some` and `root`
//! is a git repository, `zzop_git::collect` feeds real `FileNode`s (via `zzop_core::build_file_nodes`),
//! from which `zzop_metrics`' `scores`/`health`/`recommendations`/`critical`/`seams` are computed.
//!
//! Two per-file "fragment now, compose later" passes run here over data the fused pass already
//! collected — no second parse: [`late_resolve_cross_file_consumes`] re-resolves a cross-file-indirected
//! `http` CONSUME from merged constant-map fragments, and [`compose_trpc_provides`] merges tRPC router
//! fragments into whole-tree `trpc` PROVIDEs.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use zzop_core::{
    build_file_nodes, circular_from_dep_excluding, dsl::RuleTiming, is_enabled, merge_findings,
    CommonIr, DepGraph, Finding, ImportMap, IoConsume, IoFacts, IoProvide, MinimalIr, ReExport,
    DEFAULT_WEIGHTS,
};
use zzop_metrics::{
    build_coupling, build_cross_layer_co_churn, build_folder_aggregates, build_recommendations,
    compute_criticality, compute_health_index, compute_scores, compute_seams, layer_of,
    scores::types::FileKinds, BuildRecInput, CrossLayerCoChurnOptions, RecommendationGates,
    ScoresInput, DEFAULT_FOLDER_DEPTH,
};
use zzop_metrics::{
    COUPLING_TOP_PER_FILE, CRITICALITY_LIMIT, CRITICALITY_MIN_BLAST_RADIUS,
    CRITICALITY_SILENT_CHANGE_MAX, SEAMS_LIMIT, SEAMS_MIN_FILES,
};

use crate::pipeline::FileArtifact;
use crate::{AnalyzeOutput, EngineConfig};

mod compose;
mod diagnostics;
mod native_rules;

// `apply_config_mounts` is re-exported here (not just privately `use`d below) for the same reason as the
// trio above: `envelope::analyze_envelope` (Mode A) reaches it by this path too, at the structurally
// equivalent seam its own call site documents — origin-agnostic deployment topology must apply
// regardless of which assembler produced `io_provides`.
pub(crate) use compose::{
    apply_config_mounts, compose_router_mount_provides, compose_trpc_provides,
    late_resolve_cross_file_consumes,
};
// `envelope::analyze_envelope` also reaches the config-diagnostics quartet by this path (config-
// diagnostics parity with `assemble` — a `disabled_rules` typo / dead exclude filter self-reports on
// both entry points).
pub(crate) use diagnostics::{
    run_diagnostics, unmatched_global_exclude_warnings, unmatched_suppression_warnings,
    zero_packs_warning,
};
// `envelope::analyze_envelope` also imports these four native-analysis delegates by this path (same
// convention `circular_findings`'s own doc describes) — re-exported, not merely imported, so they stay
// reachable at `crate::analyze::<name>`.
pub(crate) use native_rules::{
    circular_findings, dead_candidate_findings, dep_stats_from_dep, unreachable_findings,
};

use compose::{
    apply_and_strip_global_prefix, apply_client_base_prefixes, resolve_provide_body_refs,
    resolve_wrapper_consumes,
};
use diagnostics::{
    collect_git, git_not_requested_warning, minified_files_warning, unparsed_extension_warning,
};
use native_rules::{run_callgraph_rules, run_java_provides_project_pass, run_schema_join_rules};

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
    // `build_dep_with_workspace`'s Defect-A substrate: each TS file's own re-exports (specifier +
    // `type_only`), paired with its `rel` — merged into the dep graph as real edges alongside
    // `ts_import_pairs`' bindings. Only collected for files that also participate in the dep graph
    // (`ts_import_pairs`'s own gate below), same convention as the other per-file fragment `Vec`s.
    let mut ts_re_export_pairs: Vec<(String, Vec<ReExport>)> = Vec::new();
    // `build_dep_with_workspace`'s Defect-2 substrate: each TS file's own dynamic-`import()` specifiers,
    // paired with its `rel` — merged into the dep graph as real (circular-excluded) edges alongside
    // `ts_re_export_pairs`. Same collection gate as `ts_re_export_pairs`.
    let mut ts_dynamic_import_pairs: Vec<(String, Vec<String>)> = Vec::new();
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
    let mut trpc_fragment_pairs: Vec<(String, Vec<zzop_core::ProcedureRouterFragment>)> =
        Vec::new();
    // Code-registered router-mount composition's substrate (`compose_router_mount_provides`): the
    // provide-side sibling of `trpc_fragment_pairs`, for Hono-style chained builders and cross-file
    // sub-router mounts.
    let mut router_mount_pairs: Vec<(String, Vec<zzop_core::RouterMountFragment>)> = Vec::new();
    // Wrapper-consume join's substrate (`resolve_wrapper_consumes`): per-file wrapper DEFINITION
    // fragments (exported fns whose signature carries method/path params and whose body reaches an
    // HTTP sink) and wrapper CALL fragments (call sites with captured literal args). The join
    // re-anchors HTTP consumes from wrapper internals (where egress sees only a non-literal
    // `axios.request(opts)`) to the real FE call sites.
    let mut wrapper_def_pairs: Vec<(String, Vec<zzop_core::WrapperDefFragment>)> = Vec::new();
    let mut wrapper_call_pairs: Vec<(String, Vec<zzop_core::WrapperCallFragment>)> = Vec::new();
    // Controller-prefix route composition's substrate (`compose::compose_controller_prefix_provides`):
    // each TS file's own `@Controller(RouteKey.Asset)`-shaped (dotted member-expression prefix) route
    // fragments, paired with its `rel` — resolved against the SAME merged const map `fragment_pairs`
    // feeds `late_resolve_cross_file_consumes`.
    let mut controller_prefix_route_pairs: Vec<(
        String,
        Vec<zzop_core::ControllerPrefixRouteFragment>,
    )> = Vec::new();
    // `body-shape-v1`'s DTO-resolution substrate (`compose::resolve_provide_body_refs`): each TS file's
    // own class field-shape fragments, paired with its `rel` — merged tree-wide to resolve a route
    // provide's `ProvideBodyShape.dto_ref` (the DTO class usually lives in another file than the
    // controller). Resolved once `io_provides` is final (after every provide-composition pass below).
    let mut class_shape_pairs: Vec<(String, Vec<zzop_core::ClassShapeFragment>)> = Vec::new();
    // `run_schema_join_rules`' substrate: every file's Prisma query-call-site facts, collected tree-wide
    // then sorted by `(file, line)` below to match the removed filesystem scan's own ordering.
    let mut query_call_sites: Vec<zzop_core::QueryCallSite> = Vec::new();
    // Generic entity-attribute channel — every Mode-B adapter overlay's per-file `attributes`, flattened
    // tree-wide. Shared by both `schema_usage_findings` below (dead-model/schema-churn read Symbol-keyed
    // `bound-model`/`model-churn`) and `run_callgraph_rules` (route-level auth-guard evidence), so it's
    // built once here rather than inlined at each call site.
    let attribute_store = zzop_core::AttributeStore::from_overlays(&config.adapter_overlays);
    // `schema_usage_findings`'s `SchemaUsage.identifier_counts` substrate: every file's comment/string-
    // stripped identifier tokens, unioned tree-wide — replaces that pass's own `scan_field_usage`
    // filesystem re-walk. Deliberately NOT `used_names_by_file` below: that field is AST-based
    // (`parse_local_identifier_refs`) and excludes member-property names (`obj.field`) by design (see
    // its own doc), which would make almost every model field whose only BE usage is property access
    // read as "dead" — the opposite of `scan_field_usage`'s lenient, comment/string-stripped raw-text
    // token scan this substrate must instead mirror.
    let mut field_usage_tokens: HashSet<String> = HashSet::new();
    // `unparsed_extension_warning`'s collection substrate: per extension, the TOTAL file count and up to 3
    // sample `rel`s (capped during collection, not at emission, so a huge tree never holds more than 3
    // rels per extension). `BTreeMap` keeps extension order deterministic without a separate sort.
    let mut unparsed_extensions: std::collections::BTreeMap<String, (usize, Vec<String>)> =
        std::collections::BTreeMap::new();
    // The "bring an adapter" disclosure's overlay-exclusion set: every path any `config.adapter_overlays`
    // entry declares a `FileProjection` for THAT ITSELF CARRIES A REAL FACT (`envelope::
    // overlay_file_carries_facts` — non-empty io/symbols/imports/fragments/attributes, or `is_entry`) —
    // built once, straight from config (same source the `dead-candidates` block further below reads for
    // its own `is_entry` union, not re-validated here either), so a file an adapter overlay already
    // covers with REAL data is never told it "has no native parser": the overlay IS the parser for it, and
    // both `apply_adapter_overlays` merge branches (onto an existing native artifact, or a brand-new
    // synthetic one) land that same `rel` in `artifacts` either way, so checking `config` directly here
    // tracks artifact provenance for every VALID overlay.
    //
    // Was previously ANY declared path, regardless of whether its projection carried any facts at all — a
    // bad/empty adapter (every entry's `io`/`symbols`/`imports`/fragments/`attributes` empty, `is_entry:
    // false`) still counted as "coverage" and thereby SUPPRESSED this very disclosure for files it does
    // nothing for. Narrowed to fact-carrying paths so a zero-fact "coverage" claim no longer silences the
    // disclosure (`apply_adapter_overlays` separately warns about the zero-fact overlay itself). Still not
    // exact for an overlay `apply_adapter_overlays` itself rejects (fails `validate_envelope`, e.g. a bad
    // `format` string) — that overlay's fact-carrying declared paths are excluded here even though nothing
    // actually merged, so a file behind a rejected overlay stays silently un-warned rather than reported.
    // Accepted as the same trade-off the `dead-candidates` `is_entry` union further below already makes
    // (also read straight from `config.adapter_overlays`, unvalidated) — not a new gap this change
    // introduces.
    let overlay_covered_paths: HashSet<&str> = config
        .adapter_overlays
        .iter()
        .flat_map(|overlay| overlay.files.iter())
        .filter(|file| crate::envelope::overlay_file_carries_facts(file))
        .map(|file| file.path.as_str())
        .collect();

    for artifact in artifacts {
        loc_by_path.insert(artifact.rel.clone(), artifact.loc);
        if artifact.minified_or_generated {
            minified.push(artifact.rel.clone());
        }
        // Computed once per artifact (was two separate `dispatch(...)` calls in the `else if` chain below,
        // plus now a third use for the unparsed-extension check) — `dispatch` is a pure path/extension
        // match, so caching it in a local is a free correctness-neutral simplification, not a behavior
        // change.
        let dispatch_lang = crate::dispatch::dispatch(&artifact.rel, &config.dispatch);
        if artifact.degraded {
            degraded.push(artifact.rel.clone());
        } else if dispatch_lang == Some(crate::dispatch::Language::Prisma) {
            prisma_rels.push(artifact.rel.clone());
        } else if dispatch_lang == Some(crate::dispatch::Language::JavaLexical) {
            java_rels.push(artifact.rel.clone());
        }
        // "Bring an adapter" per-extension disclosure (`unparsed_extension_warning`): the trigger is
        // `dispatch_lang.is_none()` — deliberately NOT gated on `!artifact.degraded` above. A normal-sized
        // dispatch-`None` file has `degraded: false` (no adapter to run, but nothing failed either); an
        // OVERSIZED file of the same unparsed extension short-circuits to `degraded: true` before dispatch
        // is even consulted for language selection (`pipeline::compute_fresh_artifact`'s oversized
        // branch), yet `dispatch_lang` is still `None` for it — same "no native parser exists for this
        // extension" fact, so it belongs in this count too (that file's size is a SEPARATE, already-
        // disclosed fact via `degraded`/`silent-truncation`, not a reason to hide the extension gap).
        // Extensionless files (README, Dockerfile) are deliberately excluded from v1: ambiguous by
        // construction — often config/docs, no reliable language signal to name.
        if dispatch_lang.is_none() && !overlay_covered_paths.contains(artifact.rel.as_str()) {
            if let Some(ext) = std::path::Path::new(&artifact.rel)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
            {
                if !crate::dispatch::is_non_source_extension(&ext) {
                    let entry = unparsed_extensions
                        .entry(ext)
                        .or_insert((0usize, Vec::new()));
                    entry.0 += 1;
                    if entry.1.len() < 3 {
                        entry.1.push(artifact.rel.clone());
                    }
                }
            }
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
            if !artifact.re_exports.is_empty() {
                ts_re_export_pairs.push((artifact.rel.clone(), artifact.re_exports));
            }
            if !artifact.dynamic_imports.is_empty() {
                ts_dynamic_import_pairs.push((artifact.rel.clone(), artifact.dynamic_imports));
            }
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
        if !artifact.procedure_router_fragments.is_empty() {
            trpc_fragment_pairs.push((artifact.rel.clone(), artifact.procedure_router_fragments));
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
        if !artifact.controller_prefix_route_fragments.is_empty() {
            controller_prefix_route_pairs.push((
                artifact.rel.clone(),
                artifact.controller_prefix_route_fragments,
            ));
        }
        if !artifact.class_shape_fragments.is_empty() {
            class_shape_pairs.push((artifact.rel.clone(), artifact.class_shape_fragments));
        }
        query_call_sites.extend(artifact.query_call_sites);
        field_usage_tokens.extend(artifact.field_usage_tokens);
        all_symbols.extend(artifact.symbols);
        for t in artifact.rule_timings {
            let entry = rule_time.entry(t.rule_id).or_insert((0, 0));
            entry.0 += t.nanos;
            entry.1 += t.findings;
        }
        per_file_findings.extend(artifact.findings);
    }
    // Files are collected in `artifacts`' own `rel` order (`pipeline::run_file_pass`'s invariant), so a
    // stable sort by `(file, line)` alone reproduces the removed filesystem scan's ordering exactly.
    query_call_sites.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));

    // The merged project-wide const map is computed BEFORE `late_resolve_cross_file_consumes` below
    // takes ownership of `fragment_pairs` (it only borrows here) — `compose_controller_prefix_provides`
    // needs the SAME merge (`compose::merge_const_map_fragments`'s doc) a few lines further down.
    let merged_consts = compose::merge_const_map_fragments(&fragment_pairs);
    late_resolve_cross_file_consumes(fragment_pairs, &mut io_consumes);

    // `warnings` is declared here (rather than nearer its git-related first uses further down) so both
    // the controller-prefix composer and the NestJS global-prefix transform immediately below can push
    // their own honest-degrade warnings at the seam where each rewrite is correctly scoped.
    let mut warnings: Vec<String> = Vec::new();

    // Controller-prefix route PROVIDE composition (`controller-prefix-ref-v1`) — MUST run BEFORE the
    // NestJS global-prefix apply/strip immediately below: a `@Controller(RouteKey.Asset)` controller
    // under `app.setGlobalPrefix('api')` needs BOTH transforms, in order (`RouteKey.Asset` -> `assets`
    // here, then global `/api` -> `GET /api/assets/{}` at the seam below) — this composer's output must
    // already be sitting in `io_provides` for that seam to see and prepend it, same as every other
    // per-file-composed provide.
    if !controller_prefix_route_pairs.is_empty() {
        let composed = compose::compose_controller_prefix_provides(
            controller_prefix_route_pairs,
            &merged_consts,
            &mut warnings,
        );
        io_provides.extend(composed);
    }

    // NestJS `app.setGlobalPrefix(...)` apply + strip — MUST run at exactly this seam: after the
    // per-file IO collection loop (the only producer of Nest-controller `http` provides and the
    // `nest-global-prefix` sentinels) AND after the controller-prefix composition immediately above
    // (whose resolved provides are themselves Nest-controller routes needing the same global prefix),
    // but BEFORE the Java Spring / Hono router-mount / file-convention route provide passes below all
    // append their own `http` provides. See `apply_and_strip_global_prefix`'s doc for why scope matters
    // (prefixing a non-Nest route is wrong).
    apply_and_strip_global_prefix(&mut io_provides, &mut warnings);

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
            zzop_parser_typescript::resolve_file_with_workspace(
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
            zzop_parser_typescript::resolve_file_with_workspace(
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
                zzop_parser_typescript::resolve_file_with_workspace(
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

    // Axios `baseURL` path-prefix apply + strip (`axios-defaults-base-v1`) — the CONSUME-side
    // counterpart of `apply_and_strip_global_prefix` above. MUST run here: after
    // `late_resolve_cross_file_consumes`, which fills `key` IN PLACE and preserves the `client` tag —
    // that tag is the load-bearing reason for the ordering (a late-resolved axios consume still gets
    // the prefix). Sitting after the wrapper-consume join is only "after the last consume-mutating
    // pass" hygiene: wrapper-emitted consumes carry `client: None` and are DELIBERATELY never
    // prefixed (custom wrappers stay uninterpreted — overlay territory). Must stay before
    // `io_consumes` is frozen into `MinimalIr::io` / read by any whole-tree rule
    // (`unprovided-consume`) or the cross-layer linker.
    // See `compose::apply_client_base_prefixes`'s own doc for the full placement rationale.
    apply_client_base_prefixes(&mut io_consumes, &mut warnings);

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

    // Request-body DTO resolution (`body-shape-v1`) — MUST run here, after every provide-composition
    // pass above (controller-prefix, global-prefix, tRPC, router-mount, file-convention routes) has
    // finished pushing into `io_provides`, so a prefix-ref-composed route's carried-through `body` (see
    // `compose_controller_prefix_provides`'s doc) gets its `dto_ref` resolved too, not just literal-prefix
    // routes' own directly-emitted provides.
    resolve_provide_body_refs(&mut io_provides, class_shape_pairs, &mut warnings);

    // Deployment-topology mount apply (`EngineConfig::mounts`, config-declared) — MUST run LAST among
    // provide transforms: after EVERY provide producer above (controller-prefix, global-prefix, tRPC,
    // router-mount, Java, file-convention routes via `file_routes::compose_file_convention_provides`) and
    // after body-ref resolution, so a config mount covers ALL http provides regardless of which producer
    // emitted them. Config mounts stack ON TOP of whatever code-extracted prefix (e.g. Nest
    // `setGlobalPrefix`) a provide already carries — a deployment gateway lives outside the app, so its
    // prefix is deliberately the outermost layer, applied last. Must stay before `io_provides` is sorted/
    // frozen into `MinimalIr::io` below, and before the cross-layer join (`analyze_trees`, `lib.rs`) sees
    // it. See `compose::apply_config_mounts`'s own doc for the winner-selection/validation/tripwire rules.
    apply_config_mounts(&mut io_provides, &config.mounts, &mut warnings);

    // `type_only_edges` is the ephemeral noncycle-exclusion set (never cached/serialized — see
    // `circular_from_dep_excluding`'s doc): a pair present here is contributed ONLY by edges excludable
    // from cycle detection — type-only bindings/re-exports, or a dynamic `import()` (Defect 2) — so
    // `circular_findings` below must not count it as a cycle edge even though `dep` itself (fan-in/
    // dead-exports/every other metric) still includes it.
    let (dep, type_only_edges): (DepGraph, HashSet<(String, String)>) =
        zzop_parser_typescript::build_dep_with_workspace(
            &ts_import_pairs,
            &ts_re_export_pairs,
            &ts_dynamic_import_pairs,
            &ts_paths,
            &pkg_scan.workspace_pkgs,
            &tsconfigs,
        );
    let cycles = circular_from_dep_excluding(&dep, &type_only_edges);

    let dep_stats = dep_stats_from_dep(&dep);

    // Git-history-dependent analyses. `None`/failed-collection both fall through to a default
    // (all-zero) `GitStats` and no commits — `nodes` still builds (dep-graph + LOC signal only) and
    // scores/health/recommendations/critical/seams stay empty. (`warnings` was declared earlier, at the
    // global-prefix seam.)
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
        // No `extra_entries`/exempt-set parameter exists here to thread an overlay `is_entry` union
        // through (unlike `dead_candidate_findings` below) — `unreachable_findings`/`find_unreachable`
        // already treat every `fan_in == 0` file as an implicit entry point (false-positive-safe by
        // construction), so an `is_entry`-marked overlay file with zero fan-in is already exempt without
        // any change here. Threading a real exempt set through for a fan_in > 0 case is a follow-up, not
        // addressed by this task's scope (see `dead_export_findings` below, which has no such parameter
        // either and is a second, separate follow-up).
        let t0 = profile.then(Instant::now);
        let found = unreachable_findings(&nodes, &dep);
        record_native_timing(&mut rule_time, t0, "unreachable", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "dead-candidates") {
        // `extra_entries`: package.json-referenced files (manifest entry fields + lexically-scanned
        // `scripts` path tokens) — real entry points loaded by Node/bundlers/npm directly, never via
        // `import`, so `fan_in == 0` on them is expected, not dead-code signal — UNIONED with every Mode
        // B adapter-overlay `FileProjection` marked `is_entry: true` (`EngineConfig::adapter_overlays`),
        // the overlay counterpart of a manifest entry: a framework-loaded file (SvelteKit `hooks.*`/
        // `+page`, a `.vue` route, ...) an adapter declares reachable by convention rather than import.
        // Overlays are applied post-cache (`envelope::apply_adapter_overlays`, called from `analyze_tree`
        // before this function runs) and never merged into `pkg_scan` itself (a filesystem-only scan), so
        // this reads `config.adapter_overlays` directly rather than threading a new parameter through.
        let t0 = profile.then(Instant::now);
        let mut extra_entries = pkg_scan.extra_entries.clone();
        extra_entries.extend(
            config
                .adapter_overlays
                .iter()
                .flat_map(|overlay| overlay.files.iter())
                .filter(|file| file.is_entry)
                .map(|file| file.path.clone()),
        );
        let found = dead_candidate_findings(&nodes, &dep, &extra_entries);
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
        let found = crate::pipeline::schema_usage_findings(
            root,
            &prisma_rels,
            &attribute_store,
            &field_usage_tokens,
        );
        record_native_timing(&mut rule_time, t0, "schema-usage", found.len());
        global_findings.extend(found);
    }

    // The schema x usage JOIN native rules — see `run_schema_join_rules`'s own doc.
    run_schema_join_rules(
        root,
        &prisma_rels,
        &query_call_sites,
        config,
        profile,
        &mut rule_time,
        &mut global_findings,
    );

    // Native fullstack rule: same (METHOD, path) HTTP route provided 2+ times across the tree — a
    // whole-tree pass over `io_provides` already collected above.
    if is_enabled(&config.rule_config, "duplicate-route") {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::duplicate_route_findings(&io_provides);
        record_native_timing(&mut rule_time, t0, "duplicate-route", found.len());
        global_findings.extend(found);
    }

    // Native fullstack rule: within one file, an earlier param route shadows a later literal route of
    // the same shape (see `zzop_rules_http::route_shadowing`'s module doc for the decidable subset).
    if is_enabled(&config.rule_config, "route-shadowing") {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::route_shadowing_findings(&io_provides);
        record_native_timing(&mut rule_time, t0, "route-shadowing", found.len());
        global_findings.extend(found);
    }

    // Native fullstack rule: a resolved `http` consume with no matching provide anywhere in this tree,
    // gated on this tree itself having at least one `http` provide (see
    // `zzop_rules_http::unprovided_consume`'s module doc for the zero-provides veto).
    if is_enabled(&config.rule_config, "unprovided-consume") {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::unprovided_consume_findings(&io_provides, &io_consumes);
        record_native_timing(&mut rule_time, t0, "unprovided-consume", found.len());
        global_findings.extend(found);
    }

    run_callgraph_rules(
        root,
        config,
        &attribute_store,
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
    warnings.extend(unparsed_extension_warning(&unparsed_extensions));
    let rels: Vec<&str> = loc_by_path.keys().map(String::as_str).collect();
    warnings.extend(unmatched_suppression_warnings(config, &rels));
    warnings.extend(unmatched_global_exclude_warnings(config, &rels));
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
    // has a backend but produced zero `http` provides — an unsupported/unrecognized framework signal
    // (S1). Computed here, while `io_provides`/`io_consumes`/`ts_paths`/`java_rels`/`package_import_files`
    // are still in scope.
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

    // S2 — server-framework import tripwire (provide side): a server-framework package import present
    // while extracted `http` provides stay near-zero (closes the method-call registration idiom S1's
    // decorator regex cannot see). Additive to S1 above; both may fire. Pure map lookup over
    // `package_import_files` (already a sorted `BTreeMap`/`BTreeSet`) — no disk IO, so unconditional.
    if let Some(w) =
        crate::framework_silence::server_framework_import_warning(&package_import_files, http_count)
    {
        warnings.push(w);
    }

    // S4 — http-client import tripwire (consume side): an http-CLIENT package import present while
    // extracted `http` consumes stay near-zero — the consume-side dual of S2. Additive to S1-S3 above;
    // any subset may fire together. `http_consumes_count` counts ALL extracted `http`-kind consume
    // records — keyed AND unresolved — per `client_library_import_warning`'s own doc on why. Pure map
    // lookup over `package_import_files`, no disk IO, so unconditional.
    let http_consumes_count = io_consumes.iter().filter(|c| c.kind == "http").count();
    if let Some(w) = crate::framework_silence::client_library_import_warning(
        &package_import_files,
        http_consumes_count,
    ) {
        warnings.push(w);
    }

    // S3 — committed-spec io-silence tripwire (consume side): a committed OpenAPI/Swagger spec present
    // while this tree's io stays near-zero in BOTH directions (the generated-client blind spot). The
    // `IO_NEAR_ZERO_FLOOR` precheck here mirrors the function's own internal gate (which fires only
    // when BOTH directions are near-zero), done here too so the sorted-walked-rel-list build below
    // (`loc_by_path.keys()` — same source as `file_count`, per that field's own doc) is skipped
    // entirely on any tree with healthy io in either direction (a pure BE with real provides or a
    // pure FE with keyed consumes never pays it).
    let io_provides_count = io_provides.len();
    let io_consumes_keyed_count = io_consumes.iter().filter(|c| c.key.is_some()).count();
    if io_provides_count < crate::framework_silence::IO_NEAR_ZERO_FLOOR
        && io_consumes_keyed_count < crate::framework_silence::IO_NEAR_ZERO_FLOOR
    {
        let mut all_walked_rels: Vec<String> = loc_by_path.keys().cloned().collect();
        all_walked_rels.sort();
        if let Some(w) = crate::framework_silence::committed_spec_io_silence_warning(
            root,
            &all_walked_rels,
            io_provides_count,
            io_consumes_keyed_count,
        ) {
            warnings.push(w);
        }
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

    // `root.is_dir()` gates this so it doesn't duplicate `analyze_tree`'s more specific "root does not
    // exist or is not a directory" self-report (`lib.rs`'s `scope_warnings`) — that one already states
    // the cause when the root itself is invalid, and every failure mode from an invalid root funnels
    // through `file_count == 0` too. For a root that DOES exist but simply matched no analyzable files,
    // no such self-report ran (see `lib.rs`'s "0 source files found under root" check, which only covers
    // that same case from a different angle), so this generic line still carries its own information and
    // stays.
    if file_count == 0 && root.is_dir() {
        warnings.push(
            "root produced 0 analyzable files — check the path exists and contains supported source files".to_string(),
        );
    }

    let rule_timings = profile.then(|| sort_rule_timings(rule_time));

    let ir = CommonIr {
        source: config.source_id.clone(),
        // Multiple parser frontends (TypeScript + Prisma, v1 scope) are fused into one tree-wide IR here —
        // no single `parser` id is accurate the way it is for a single-frontend `build_common_ir` call, so
        // this is a zzop-only tag naming the fused engine itself rather than one frontend.
        parser: "engine".to_string(),
        ir: MinimalIr {
            dep,
            symbols: all_symbols,
            loc: loc_by_path,
            io,
        },
    };

    let coverage = crate::CoverageCensus::compute(file_count, &ir, degraded.len());

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
        coverage,
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
