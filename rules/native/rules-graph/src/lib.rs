//! zzop-rules-graph — native whole-graph rules that operate over a repo's dependency/call graph, git-free.
//!
//! ## Module map
//! - [`http_scan`]: call-graph-BFS HTTP scanners (`scan_unsafe_read_endpoint`, `scan_non_idempotent_write`).
//! - [`circular`]: Finding-shaping for `"circular"` (the algorithm itself lives in `zzop_core::graph`).
//! - [`unreachable`]: closed "dead island" file detection.
//! - [`dead_candidates`]: fanIn == 0 candidate dead files.
//! - [`dead_exports`]: symbol-level dead-export detection.
//! - [`duplicate_route`]: same HTTP route registered 2+ times.
//! - [`route_shadowing`]: an earlier param route shadows a later literal route in the same file.
//! - [`mutating_route_no_auth`]: a mutating route's handler never reaches an auth-guard callee via BFS.
//! - [`unprovided_consume`]: a resolved `http` consume with no matching provide anywhere in the analysis.
//! - [`cross_layer`]: rules over the MULTI-TREE `CrossLayerResult` join — the whole-analysis counterpart to
//!   every module above (see its own doc for the full list).
//!
//! Every rule body here depends on `zzop-core` only.

pub mod circular;
pub mod cross_layer;
pub mod dead_candidates;
pub mod dead_exports;
pub mod duplicate_route;
pub mod http_scan;
pub mod mutating_route_no_auth;
pub mod route_shadowing;
pub mod unprovided_consume;
pub mod unreachable;

use zzop_core::{register_native_analysis_stub, RuleRegistry, Severity};

/// Registers every native analysis id whose implementation lives in this crate (see `rules/README.md`'s
/// "Adding a rule" section); `zzop_engine::register_all_native` composes this with the other crates' own.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    let analyses: &[(&str, Severity)] = &[
        ("circular", Severity::Warning),
        ("unreachable", Severity::Info),
        ("dead-candidates", Severity::Info),
        ("dead-exports", Severity::Info),
        ("duplicate-route", Severity::Warning),
        ("unsafe-read-endpoint", Severity::Warning),
        ("non-idempotent-write", Severity::Warning),
        ("route-shadowing", Severity::Warning),
        ("mutating-route-no-auth", Severity::Info),
        ("unprovided-consume", Severity::Info),
        ("cross-layer/unconsumed-endpoint", Severity::Info),
        ("cross-layer/method-mismatch", Severity::Warning),
        ("cross-layer/version-skew", Severity::Warning),
        ("cross-layer/path-near-miss", Severity::Info),
        ("cross-layer/shared-db-table", Severity::Warning),
        ("cross-layer/duplicate-route", Severity::Warning),
        ("cross-layer/external-shadow-internal", Severity::Warning),
        ("cross-layer/external-secret-in-url", Severity::Warning),
        (
            "cross-layer/external-duplicated-integration",
            Severity::Warning,
        ),
        ("cross-layer/external-host-fanout", Severity::Info),
        ("cross-layer/external-base-url-drift", Severity::Info),
        ("cross-layer/external-version-inconsistent", Severity::Info),
        ("cross-layer/external-ip-literal", Severity::Warning),
        ("cross-layer/ambiguous-consume", Severity::Warning),
        (
            "cross-layer/unconsumed-mutation-endpoint",
            Severity::Warning,
        ),
        ("cross-layer/unprovided-mutation-call", Severity::Warning),
        ("cross-layer/route-shadowing", Severity::Warning),
        ("cross-layer/unresolved-consume-ratio", Severity::Info),
        ("cross-layer/sdk-import-no-visible-consume", Severity::Info),
        ("cross-layer/unconsumed-procedure", Severity::Info),
    ];
    for &(id, default_severity) in analyses {
        register_native_analysis_stub(registry, id, default_severity);
    }
}

pub use circular::circular_findings;
pub use cross_layer::{
    ambiguous_consume_findings, cross_layer_duplicate_route_findings,
    cross_tree_route_shadowing_findings, external_base_url_drift_findings,
    external_duplicated_integration_findings, external_host_fanout_findings,
    external_ip_literal_findings, external_secret_in_url_findings,
    external_shadow_internal_findings, external_version_inconsistent_findings,
    method_mismatch_findings, path_near_miss_findings, sdk_import_no_visible_consume_findings,
    shared_db_table_findings, unconsumed_endpoint_findings, unconsumed_mutation_endpoint_findings,
    unconsumed_procedure_findings, unprovided_mutation_call_findings,
    unresolved_consume_ratio_findings, version_skew_findings, HttpProvideSite, PackageImportSite,
};
pub use dead_candidates::{dead_candidate_findings, find_dead_candidates, DEAD_MAX_CHANGES};
pub use dead_exports::{
    dead_export_findings, find_dead_exports, DeadExport, DeadExportCandidate, DeadExportInputFile,
    DeadExportReason,
};
pub use duplicate_route::duplicate_route_findings;
pub use http_scan::{
    scan_non_idempotent_write, scan_unsafe_read_endpoint, ScanNonIdempotentWriteInput,
    ScanUnsafeReadEndpointInput,
};
pub use mutating_route_no_auth::{
    scan_mutating_route_no_auth, ScanMutatingRouteNoAuthInput, DEFAULT_AUTH_GUARD_PATTERN,
};
pub use route_shadowing::route_shadowing_findings;
pub use unprovided_consume::unprovided_consume_findings;
pub use unreachable::{find_unreachable, unreachable_findings, UnreachableFile};
