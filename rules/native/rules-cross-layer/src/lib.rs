//! zzop-rules-cross-layer — native rules over the MULTI-TREE `CrossLayerResult` join, git-free.
//!
//! ## Module map
//! - [`cross_layer`]: rules over the MULTI-TREE `CrossLayerResult` join — the whole-analysis counterpart
//!   to the single-tree rules in `zzop-rules-graph`/`zzop-rules-http` (see its own doc for the full list).
//!
//! Every rule body here depends on `zzop-core` only.

pub mod cross_layer;

use zzop_core::{register_native_analysis_stub, RuleRegistry};

/// Registers every native analysis id whose implementation lives in this crate (see `rules/README.md`'s
/// "Adding a rule" section); `zzop_engine::register_all_native` composes this with the other crates' own.
/// Ids only — a finding's severity is set where the finding is built (`cross_layer::*`), so there is no
/// second copy here to drift from it.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    for id in [
        "cross-layer/unconsumed-endpoint",
        "cross-layer/method-mismatch",
        "cross-layer/version-skew",
        "cross-layer/path-near-miss",
        "cross-layer/route-near-miss",
        "cross-layer/prefix-drift",
        "cross-layer/db-table-name-in-multiple-sources",
        "cross-layer/duplicate-route",
        "cross-layer/external-shadow-internal",
        "cross-layer/external-secret-in-url",
        "cross-layer/external-host-in-multiple-sources",
        "cross-layer/external-host-fanout",
        "cross-layer/external-base-url-drift",
        "cross-layer/external-version-inconsistent",
        "cross-layer/external-ip-literal",
        "cross-layer/ambiguous-consume",
        "cross-layer/all-consumes-unjoined",
        "cross-layer/unconsumed-mutation-endpoint",
        "cross-layer/unprovided-mutation-call",
        "cross-layer/route-shadowing",
        "cross-layer/unresolved-consume-ratio",
        "cross-layer/untraced-client-import-no-visible-consume",
        "cross-layer/unconsumed-procedure",
        "cross-layer/body-field-drift",
        "cross-layer/sensitive-response-field",
        "cross-layer/retrying-write-no-idempotency",
        "cross-layer/unknown-verb-route",
    ] {
        register_native_analysis_stub(registry, id);
    }
}

/// This crate's machine-readable sightline declarations (`zzop_core::RuleSightline`) — the rules
/// whose trigger is a single-producer fact: `cross_layer::retrying_write_no_idempotency`
/// (`IoConsume::retry_configured`) and `cross_layer::sensitive_response_field`
/// (`IoProvide::response`); each declaration lives WITH its rule, composed with the other crates'
/// own by `zzop_engine::rule_sightlines`, the same aggregator shape as [`register_native_analyses`].
pub fn rule_sightlines() -> Vec<zzop_core::RuleSightline> {
    let mut out = cross_layer::retrying_write_no_idempotency::sightlines();
    out.extend(cross_layer::sensitive_response_field::sightlines());
    out
}

pub use cross_layer::external_secret_in_url::SECRET_PARAM_NAMES;
pub use cross_layer::retrying_write_no_idempotency::{
    IDEMPOTENCY_GUARDED_ATTR, RETRY_WITNESS_EXTENSIONS,
};
pub use cross_layer::sensitive_response_field::{
    RESPONSE_WITNESS_EXTENSIONS, SENSITIVE_RESPONSE_FIELD_EXACT,
    SENSITIVE_RESPONSE_FIELD_SUBSTRINGS, SENSITIVE_RESPONSE_FIELD_SUFFIXES,
};
pub use cross_layer::unconsumed_endpoint::EXTERNALLY_FETCHED_PATHS;
pub use cross_layer::CROSS_LAYER_WRITE_METHODS;
pub use cross_layer::VERSION_SEGMENT_PATTERN;
pub use cross_layer::{
    all_consumes_unjoined_findings, ambiguous_consume_findings, body_field_drift_findings,
    cross_layer_duplicate_route_findings, cross_tree_route_shadowing_findings,
    external_base_url_drift_findings, external_duplicated_integration_findings,
    external_host_fanout_findings, external_ip_literal_findings, external_secret_in_url_findings,
    external_shadow_internal_findings, external_version_inconsistent_findings,
    is_trpc_mount_route_path, majority_unresolved_http_sources, method_mismatch_findings,
    path_near_miss_findings, prefix_drift_findings, retain_non_subsumed,
    retain_non_subsumed_sources, retrying_write_no_idempotency_findings, route_near_miss_findings,
    sdk_import_no_visible_consume_findings, sensitive_response_field_findings,
    shared_db_table_findings, trpc_mount_route_suppression_notes, unconsumed_endpoint_findings,
    unconsumed_mutation_endpoint_findings, unconsumed_procedure_findings,
    unknown_verb_route_findings, unprovided_mutation_call_findings,
    unresolved_consume_ratio_findings, version_skew_findings, HttpProvideSite, PackageImportSite,
    ResponseProvideSite, RetrySite, SensitiveResponseVocab, UnknownVerbRouteSite,
};
