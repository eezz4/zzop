//! The [`Collected`] output type for `super::collect` — split out from the `collect()` function
//! itself purely to keep both files under the line-count ratchet; no logic lives here.

use std::collections::{HashMap, HashSet};

use zzop_core::{Finding, ImportMap, IoConsume, IoProvide, ReExport};

use crate::pipeline::{GoModuleMap, JavaIndex, RustWorkspaceMap};

/// Every per-file substrate the fused pass collected, bucketed for the phases below. Field docs are
/// preserved from the pre-split monolithic `assemble` — see each field for why it exists and who
/// consumes it.
pub(in crate::analyze::assemble) struct Collected {
    pub(in crate::analyze::assemble) file_count: usize,
    pub(in crate::analyze::assemble) per_file_findings: Vec<Finding>,
    pub(in crate::analyze::assemble) all_symbols: Vec<zzop_core::ir::SourceSymbol>,
    pub(in crate::analyze::assemble) loc_by_path: HashMap<String, u32>,
    pub(in crate::analyze::assemble) ts_import_pairs: Vec<(String, ImportMap)>,
    /// `build_dep_with_workspace`'s Defect-A substrate: each TS file's own re-exports (specifier +
    /// `type_only`), paired with its `rel` — merged into the dep graph as real edges alongside
    /// `ts_import_pairs`' bindings. Only collected for files that also participate in the dep graph
    /// (`ts_import_pairs`'s own gate below), same convention as the other per-file fragment `Vec`s.
    pub(in crate::analyze::assemble) ts_re_export_pairs: Vec<(String, Vec<ReExport>)>,
    /// `build_dep_with_workspace`'s Defect-2 substrate: each TS file's own dynamic-`import()` specifiers,
    /// paired with its `rel` — merged into the dep graph as real (circular-excluded) edges alongside
    /// `ts_re_export_pairs`. Same collection gate as `ts_re_export_pairs`.
    pub(in crate::analyze::assemble) ts_dynamic_import_pairs: Vec<(String, Vec<String>)>,
    pub(in crate::analyze::assemble) ts_paths: HashSet<String>,
    pub(in crate::analyze::assemble) degraded: Vec<String>,
    /// `pipeline::eval_packs`' minified/generated skip — a separate list from `degraded` (see
    /// `pipeline::FileArtifact::minified_or_generated`'s doc), surfaced as one aggregate `warnings`
    /// entry (`minified_files_warning`) rather than a per-file entry.
    pub(in crate::analyze::assemble) minified: Vec<String>,
    pub(in crate::analyze::assemble) io_provides: Vec<IoProvide>,
    pub(in crate::analyze::assemble) io_consumes: Vec<IoConsume>,
    /// `dead-exports`' per-file "used names" input — collected unconditionally (cheap, already cached by
    /// the fused pass); the `is_enabled` gate in `super::rules` decides whether the more expensive
    /// second pass runs.
    pub(in crate::analyze::assemble) used_names_by_file: HashMap<String, Vec<String>>,
    /// `schema-usage`'s whole-tree input: every non-degraded Prisma-dispatched file (a degraded schema
    /// parses to zero models, so it's excluded).
    pub(in crate::analyze::assemble) prisma_rels: Vec<String>,
    /// `run_java_provides_project_pass`'s whole-corpus input: every java-dispatched file's rel path,
    /// collected unconditionally — the project pass needs EVERY java file, not just the ones whose own
    /// per-file pass emitted a provide, since a file with no routes of its own (e.g. a prefix-constants
    /// file) still needs to be present for its constants to resolve.
    pub(in crate::analyze::assemble) java_rels: Vec<String>,
    /// `EngineConfig::profile_rules` reduce step: each `FileArtifact` carries its own file-local
    /// `rule_timings`, summed per `rule_id` in the loop below. Stays empty when profiling is off.
    pub(in crate::analyze::assemble) rule_time: HashMap<String, (u128, usize)>,
    /// Per-package (non-relative specifier) importing-file sets — summarized into
    /// `AnalyzeOutput::package_imports` for `cross-layer/sdk-import-no-visible-consume` (the tree IR
    /// drops package imports during dep resolution, so this is the one place the data still exists).
    pub(in crate::analyze::assemble) package_import_files:
        std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// Late consume resolution's substrate: each TS file's own constant-map fragment, paired with its
    /// `rel` so the merge below can sort by path for deterministic first-writer-wins resolution of a key
    /// duplicated across files.
    pub(in crate::analyze::assemble) fragment_pairs: Vec<(String, HashMap<String, String>)>,
    /// tRPC PROVIDE composition's substrate (`compose_trpc_provides`): each TS file's own tRPC
    /// router-fragment shape, paired with its `rel`. Composed directly into `IoProvide`s rather than
    /// re-keying an `IoConsume` (see `crate::io`'s module doc).
    pub(in crate::analyze::assemble) trpc_fragment_pairs:
        Vec<(String, Vec<zzop_core::ProcedureRouterFragment>)>,
    /// Code-registered router-mount composition's substrate (`compose_router_mount_provides`): the
    /// provide-side sibling of `trpc_fragment_pairs`, for Hono-style chained builders and cross-file
    /// sub-router mounts.
    pub(in crate::analyze::assemble) router_mount_pairs:
        Vec<(String, Vec<zzop_core::RouterMountFragment>)>,
    /// Wrapper-consume join's substrate (`resolve_wrapper_consumes`): per-file wrapper DEFINITION
    /// fragments (exported fns whose signature carries method/path params and whose body reaches an
    /// HTTP sink) and wrapper CALL fragments (call sites with captured literal args). The join
    /// re-anchors HTTP consumes from wrapper internals (where egress sees only a non-literal
    /// `axios.request(opts)`) to the real FE call sites.
    pub(in crate::analyze::assemble) wrapper_def_pairs:
        Vec<(String, Vec<zzop_core::WrapperDefFragment>)>,
    pub(in crate::analyze::assemble) wrapper_call_pairs:
        Vec<(String, Vec<zzop_core::WrapperCallFragment>)>,
    /// Controller-prefix route composition's substrate (`compose::compose_controller_prefix_provides`):
    /// each TS file's own `@Controller(RouteKey.Asset)`-shaped (dotted member-expression prefix) route
    /// fragments, paired with its `rel` — resolved against the SAME merged const map `fragment_pairs`
    /// feeds `late_resolve_cross_file_consumes`.
    pub(in crate::analyze::assemble) controller_prefix_route_pairs:
        Vec<(String, Vec<zzop_core::ControllerPrefixRouteFragment>)>,
    /// `body-shape-v1`'s DTO-resolution substrate (`compose::resolve_provide_body_refs`): each TS file's
    /// own class field-shape fragments, paired with its `rel` — merged tree-wide to resolve a route
    /// provide's `ProvideBodyShape.dto_ref` (the DTO class usually lives in another file than the
    /// controller). Resolved once `io_provides` is final (after every provide-composition pass below).
    pub(in crate::analyze::assemble) class_shape_pairs:
        Vec<(String, Vec<zzop_core::ClassShapeFragment>)>,
    /// `run_schema_join_rules`' substrate: every file's Prisma query-call-site facts, sorted by
    /// `(file, line)` to match the removed filesystem scan's own ordering.
    pub(in crate::analyze::assemble) query_call_sites: Vec<zzop_core::QueryCallSite>,
    /// `schema_usage_findings`'s `SchemaUsage.identifier_counts` substrate: every file's comment/string-
    /// stripped identifier tokens, unioned tree-wide — replaces that pass's own `scan_field_usage`
    /// filesystem re-walk. Deliberately NOT `used_names_by_file` above: that field is AST-based
    /// (`parse_local_identifier_refs`) and excludes member-property names (`obj.field`) by design (see
    /// its own doc), which would make almost every model field whose only BE usage is property access
    /// read as "dead" — the opposite of `scan_field_usage`'s lenient, comment/string-stripped raw-text
    /// token scan this substrate must instead mirror.
    pub(in crate::analyze::assemble) field_usage_tokens: HashSet<String>,
    /// `unparsed_extension_warning`'s collection substrate: per extension, the TOTAL file count and up to
    /// 3 sample `rel`s (capped during collection, not at emission, so a huge tree never holds more than
    /// 3 rels per extension). `BTreeMap` keeps extension order deterministic without a separate sort.
    pub(in crate::analyze::assemble) unparsed_extensions:
        std::collections::BTreeMap<String, (usize, Vec<String>)>,
    /// Task 6's workspace-member manifest scan (`crate::pipeline::scan_rust_workspace`), built once up
    /// front in `collect` (before the artifact-consuming loop) and reused by both the Rust package-census
    /// F5 drain here AND — passed through unchanged — `super::provides`' router-mount resolver closure and
    /// `super::dep_graph::merge_rust_dep_edges`, so every Rust-side resolution consults the SAME map.
    pub(in crate::analyze::assemble) rust_workspace: RustWorkspaceMap,
    /// Task 4's `go.mod` module-manifest scan (`crate::pipeline::scan_go_modules`), built once up front
    /// in `collect` (before the artifact-consuming loop) and reused by both the Go package-census F5
    /// drain here AND — passed through unchanged — `super::dep_graph::merge_go_dep_edges`, so every
    /// Go-side resolution consults the SAME map. Unlike `rust_workspace`, no `super::provides` router-
    /// mount resolver branch needs this (see `merge_go_dep_edges`'s doc for why cross-file Go router
    /// mounts do not need this map in v1).
    pub(in crate::analyze::assemble) go_modules: GoModuleMap,
    /// Task 4's `(package, type)` -> file scan (`crate::pipeline::scan_java_index`), built once up front
    /// in `collect` (before the artifact-consuming loop) and reused by both the Java package-census F5
    /// drain here AND `super::dep_graph::merge_java_dep_edges`, so every Java-side resolution consults the
    /// SAME index — mirrors `go_modules`'s own dual-consumer doc.
    pub(in crate::analyze::assemble) java_index: JavaIndex,
}
