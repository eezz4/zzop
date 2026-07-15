//! zzop-rules-cross-layer — native rules over the MULTI-TREE `CrossLayerResult` join, git-free.
//!
//! ## Module map
//! - [`cross_layer`]: rules over the MULTI-TREE `CrossLayerResult` join — the whole-analysis counterpart
//!   to the single-tree rules in `zzop-rules-graph`/`zzop-rules-http` (see its own doc for the full list).
//!
//! Every rule body here depends on `zzop-core` only.

pub mod cross_layer;

use zzop_core::{register_native_analysis_stub, RuleRegistry, Severity};

/// Registers every native analysis id whose implementation lives in this crate (see `rules/README.md`'s
/// "Adding a rule" section); `zzop_engine::register_all_native` composes this with the other crates' own.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    let analyses: &[(&str, Severity)] = &[
        ("cross-layer/unconsumed-endpoint", Severity::Info),
        ("cross-layer/method-mismatch", Severity::Warning),
        ("cross-layer/version-skew", Severity::Warning),
        ("cross-layer/path-near-miss", Severity::Info),
        ("cross-layer/route-near-miss", Severity::Info),
        ("cross-layer/prefix-drift", Severity::Info),
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
        ("cross-layer/body-field-drift", Severity::Warning),
    ];
    for &(id, default_severity) in analyses {
        register_native_analysis_stub(registry, id, default_severity);
    }
}

pub use cross_layer::{
    ambiguous_consume_findings, body_field_drift_findings, cross_layer_duplicate_route_findings,
    cross_tree_route_shadowing_findings, external_base_url_drift_findings,
    external_duplicated_integration_findings, external_host_fanout_findings,
    external_ip_literal_findings, external_secret_in_url_findings,
    external_shadow_internal_findings, external_version_inconsistent_findings,
    majority_unresolved_http_sources, method_mismatch_findings, path_near_miss_findings,
    prefix_drift_findings, retain_non_subsumed, route_near_miss_findings,
    sdk_import_no_visible_consume_findings, shared_db_table_findings,
    trpc_mount_route_suppression_notes, unconsumed_endpoint_findings,
    unconsumed_mutation_endpoint_findings, unconsumed_procedure_findings,
    unprovided_mutation_call_findings, unresolved_consume_ratio_findings, version_skew_findings,
    HttpProvideSite, PackageImportSite,
};
