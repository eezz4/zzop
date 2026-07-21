//! Phase 2: whole-tree PROVIDE/CONSUME composition — every "fragment now, compose later" pass over
//! `super::collect::Collected`'s fragment substrates, in the exact seam order the comments below pin
//! (a provide-composition pass reading `io_provides` mid-stream, or a consume-composition pass reading
//! `io_consumes` mid-stream, must run in this order for correctness — see each block's own doc).

use std::collections::HashMap;

use zzop_core::{IoConsume, IoProvide};

use crate::analyze::compose::{
    apply_and_strip_global_prefix, apply_client_base_prefixes, apply_config_mounts,
    compose_controller_prefix_provides, compose_router_mount_provides, compose_trpc_provides,
    late_resolve_cross_file_consumes, merge_const_map_fragments, resolve_provide_body_refs,
    resolve_wrapper_consumes,
};
use crate::analyze::native_rules::{
    run_csharp_provides_project_pass, run_java_provides_project_pass,
};
use crate::pipeline::{GoModuleMap, PackageJsonScan, RustWorkspaceMap};
use crate::EngineConfig;

use super::helpers::{
    find_go_mount_target, go_fragment_dirs, is_go_source_ext, is_python_source_ext,
    is_rust_source_ext, resolve_go_import_package_dir, resolve_python_import, resolve_rust_import,
};
use super::orm::resolve_orm_entity_consumes;

/// Every output the provide/consume composition seam produces, consumed by the phases after it
/// (`super::dep_graph` needs `pkg_scan`/`tsconfigs` too — the workspace-aware resolver both the dep
/// graph AND `dead-exports` need).
pub(super) struct ProvidesResult {
    pub(super) io_provides: Vec<IoProvide>,
    pub(super) io_consumes: Vec<IoConsume>,
    pub(super) warnings: Vec<String>,
    pub(super) attribute_store: zzop_core::AttributeStore,
    pub(super) pkg_scan: PackageJsonScan,
    pub(super) tsconfigs: std::collections::BTreeMap<String, zzop_parser_typescript::TsconfigPaths>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compose(
    root: &std::path::Path,
    config: &EngineConfig,
    loc_by_path: &HashMap<String, u32>,
    ts_paths: &std::collections::HashSet<String>,
    java_rels: &[String],
    csharp_rels: &[String],
    all_symbols: &[zzop_core::ir::SourceSymbol],
    mut io_provides: Vec<IoProvide>,
    mut io_consumes: Vec<IoConsume>,
    fragment_pairs: Vec<(String, HashMap<String, String>)>,
    trpc_fragment_pairs: Vec<(String, Vec<zzop_core::ProcedureRouterFragment>)>,
    router_mount_pairs: Vec<(String, Vec<zzop_core::RouterMountFragment>)>,
    wrapper_def_pairs: Vec<(String, Vec<zzop_core::WrapperDefFragment>)>,
    wrapper_call_pairs: Vec<(String, Vec<zzop_core::WrapperCallFragment>)>,
    controller_prefix_route_pairs: Vec<(String, Vec<zzop_core::ControllerPrefixRouteFragment>)>,
    class_shape_pairs: Vec<(String, Vec<zzop_core::ClassShapeFragment>)>,
    rust_workspace: &RustWorkspaceMap,
    go_modules: &GoModuleMap,
) -> ProvidesResult {
    // The merged project-wide const map is computed BEFORE `late_resolve_cross_file_consumes` below takes ownership of
    // `fragment_pairs` (it only borrows here) — `compose_controller_prefix_provides` needs the SAME merge (`compose::merge_const_map_fragments`'s doc) a few lines further down.
    let merged_consts = merge_const_map_fragments(&fragment_pairs);
    late_resolve_cross_file_consumes(fragment_pairs, &mut io_consumes);

    // ORM entity-reference consumes (`db-table`, `key: None`, `raw: <model type>`) — TypeORM
    // `@InjectRepository(X)` and GORM `db.Model(&X{})` both reference a model by TYPE, whose physical table
    // lives with the model's own definition elsewhere in the tree. Resolve each against the tree-wide
    // type -> table-key index built from the model provides' `symbol` — runs here (after the per-file IO
    // collection populated both sides, before the cross-layer join reads `io_consumes`) so a resolved
    // consume joins its provide like any Prisma-keyed one.
    resolve_orm_entity_consumes(&io_provides, &mut io_consumes);

    // `warnings` is declared here (rather than nearer its git-related first uses in `super::dep_graph`)
    // so both the controller-prefix composer and the NestJS global-prefix transform immediately below
    // can push their own honest-degrade warnings at the seam where each rewrite is correctly scoped.
    let mut warnings: Vec<String> = Vec::new();

    // Controller-prefix route PROVIDE composition (`controller-prefix-ref-v1`) — MUST run BEFORE the
    // NestJS global-prefix apply/strip immediately below: a `@Controller(RouteKey.Asset)` controller
    // under `app.setGlobalPrefix('api')` needs BOTH transforms, in order (`RouteKey.Asset` -> `assets`
    // here, then global `/api` -> `GET /api/assets/{}` at the seam below) — this composer's output must
    // already be sitting in `io_provides` for that seam to see and prepend it, same as every other
    // per-file-composed provide.
    if !controller_prefix_route_pairs.is_empty() {
        let composed = compose_controller_prefix_provides(
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
        run_java_provides_project_pass(root, java_rels, &mut io_provides);
    }

    // Whole-corpus C# ASP.NET Core HTTP-provides resolution — the C# twin of the Java pass just above, resolving non-literal route
    // CONSTANTS (`[HttpGet(Routes.List)]`) across files and REPLACING the per-file C# `http` provides wholesale (`run_csharp_provides_project_pass`'s doc). A no-op when empty.
    if !csharp_rels.is_empty() {
        run_csharp_provides_project_pass(root, csharp_rels, &mut io_provides);
    }

    // Workspace-package manifest scan — hoisted above `build_dep` because `workspace_pkgs` also feeds
    // cross-package import resolution (`build_dep_with_workspace`): a monorepo import like
    // `import { x } from '@scope/pkg-b'` names a workspace package, not an npm dependency, and every
    // whole-graph analysis downstream of `dep` needs that edge to exist.
    let pkg_scan =
        crate::pipeline::package_json_entries(root, loc_by_path.keys().cloned(), ts_paths);
    // tsconfig `paths`/`baseUrl` alias collection: a monorepo import like `import { x } from '@/features/y'` remapped by
    // `compilerOptions.paths` needs this to become a real dep-graph edge instead of looking external/orphaned.
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
                ts_paths,
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
    // Also composes producer-judged attributes riding the same fragments (e.g. a recognized Express
    // middleware guard — `zzop_parser_typescript::adapters::router_mounts`'s `express-middleware-v1`
    // vocabulary) into `native_attrs`, fed into `AttributeStore::from_parts` below alongside every
    // Mode-B overlay's own `attributes`.
    let mut native_attrs: Vec<zzop_core::Attribute> = Vec::new();
    if !router_mount_pairs.is_empty() {
        // Built from a borrow, BEFORE `router_mount_pairs` moves into `compose_router_mount_provides`
        // below — the Go branch inside the closure needs to search fragment names across every file in
        // a resolved package directory, and `router_mount_pairs` is the only substrate that has them
        // (see `go_fragment_dirs`'s own doc for why this can't instead live inside the composer).
        let go_dirs = go_fragment_dirs(&router_mount_pairs);
        let (composed, attrs) =
            compose_router_mount_provides(router_mount_pairs, |specifier, from_file, ident| {
                if loc_by_path.contains_key(specifier) {
                    return Some(specifier.to_string());
                }
                // Python `include_router` mounts (`from_file` a `.py`/`.pyi`) resolve via the Python
                // candidate builder rather than the TS resolver — see `resolve_python_import`'s doc for
                // why this is a separate branch, not a fork of `resolve_file_with_workspace` itself.
                // `original: None`: `RouterMountEntry::Mount` carries only the module `specifier` (never
                // the imported name's own `ImportBinding::original`), so only the plain-module candidates
                // (`<base>.py`/`<base>/__init__.py`) are tried — covers the common one-router-per-module
                // layout (`router = APIRouter()` defined directly in the mounted module).
                if is_python_source_ext(from_file) {
                    return resolve_python_import(specifier, None, from_file, ts_paths);
                }
                // axum `.nest("/api", child)`/`.merge(child)` mounts (`from_file` a `.rs`): `specifier`
                // is the mounted ident's own FULL import specifier (`RouterMountEntry::Mount::specifier`
                // — see `zzop_parser_rust::adapters::axum`'s doc), so `resolve_rust_import` is called
                // directly with it, mirroring `is_rust_source_ext`'s sibling branch above exactly.
                if is_rust_source_ext(from_file) {
                    return resolve_rust_import(specifier, from_file, ts_paths, rust_workspace);
                }
                // gin cross-package mounts (`from_file` a `.go`): `specifier` is a Go IMPORT PATH
                // (`RouterMountEntry::Mount::specifier` — see `zzop_parser_go::adapters::gin`'s doc),
                // resolving to a PACKAGE DIRECTORY rather than a single file
                // (`resolve_go_import_package_dir`'s own doc) — many `.go` files can live there, so the
                // mount's own `ident` (this closure's 3rd parameter, unused by every branch above) picks
                // the ONE file whose fragment set actually names it (`go_dirs`/`find_go_mount_target`,
                // built above from the same `router_mount_pairs` this composer consumes). Unresolvable
                // (no governing module, external import, or no fragment names `ident` anywhere in the
                // directory) returns `None` — same conservative "skip the subtree" every other language
                // gets. A gin same-file group/verb join never reaches this branch at all: it resolves
                // entirely through the `specifier: None` path in `find_child` (`gin`'s `Group` prefix is
                // a local binding, never an import). See `merge_go_dep_edges`'s doc (dep_graph.rs) for
                // the related dep-graph-side Go resolution this closure has no analogue of (that one
                // fans out to every file in the directory; this one must pick exactly one).
                if is_go_source_ext(from_file) {
                    let dir = resolve_go_import_package_dir(specifier, from_file, go_modules)?;
                    return find_go_mount_target(&go_dirs, &dir, ident).map(str::to_string);
                }
                zzop_parser_typescript::resolve_file_with_workspace(
                    specifier,
                    from_file,
                    ts_paths,
                    &pkg_scan.workspace_pkgs,
                    &tsconfigs,
                )
            });
        io_provides.extend(composed);
        native_attrs = attrs;
    }

    // Generic entity-attribute channel — native producer judgments (above, e.g. a recognized Express
    // middleware guard) merged with every Mode-B adapter overlay's per-file `attributes`, flattened
    // tree-wide (overlay wins on a target+key collision — see `AttributeStore::from_parts`'s doc).
    // Built here (AFTER router-mount composition, which is the only native attribute producer today)
    // rather than at the top of this function — a pure ordering move, overlay-only behavior unchanged.
    // Shared by both `schema_usage_findings` (dead-model/schema-churn read Symbol-keyed
    // `bound-model`/`model-churn`) and `run_callgraph_rules` (route-level auth-guard evidence) in
    // `super::rules`.
    let attribute_store =
        zzop_core::AttributeStore::from_parts(native_attrs, &config.adapter_overlays);

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
                    ts_paths,
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
            all_symbols,
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

    ProvidesResult {
        io_provides,
        io_consumes,
        warnings,
        attribute_store,
        pkg_scan,
        tsconfigs,
    }
}
