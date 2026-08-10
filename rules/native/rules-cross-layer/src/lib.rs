//! zzop-rules-cross-layer — native rules over the MULTI-TREE `CrossLayerResult` join, git-free.
//!
//! ## Module map
//! - [`cross_layer`]: rules over the MULTI-TREE `CrossLayerResult` join — the whole-analysis counterpart
//!   to the single-tree rules in `zzop-rules-graph`/`zzop-rules-http` (see its own doc for the full list).
//!
//! Every rule body here depends on `zzop-core` only.

pub mod cross_layer;

use zzop_core::rule_channels::reads::{
    DB_TABLE_CONSUMES, DB_TABLE_PROVIDES, HTTP_CONSUMES, HTTP_PROVIDES, TRPC_CONSUMES,
    TRPC_PROVIDES,
};
use zzop_core::{
    declare_native_rule_channels, register_native_analysis_stub, NativeRuleChannels, RuleIoChannel,
    RuleRegistry,
};

/// Every native analysis id this crate owns, each on one row with the cross-layer io channels its rule
/// body reads (`zzop_core::rule_channels`' mechanism doc holds the contract). ONE table, two readers —
/// [`register_native_analyses`] and [`native_rule_channels`] — so an id cannot be registered without a
/// channel statement, and the statement cannot drift from the id it describes.
///
/// Ids only otherwise — a finding's severity is set where the finding is built (`cross_layer::*`), so
/// there is no second copy here to drift from it.
///
/// A row states which channel the rule's INPUT is drawn from, never what an empty channel does to its
/// output: `unconsumed-endpoint` goes silent without http provides while `unprovided-mutation-call`
/// floods, and both declare the same channel. Several rows are fed a PRE-FILTERED input by the engine
/// (`cross_layer_findings::partition::http_provide_sites`, `join_maps::http_consume_totals`, ...) and so
/// name no io kind in their own module; each of those is pinned by name against the pre-filter's own
/// source in `rule_contracts::rule_channels`, never silently skipped.
const NATIVE_ANALYSES: &[(&str, &[RuleIoChannel])] = &[
    (
        "cross-layer/unconsumed-endpoint",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    (
        "cross-layer/method-mismatch",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    ("cross-layer/version-skew", &[HTTP_PROVIDES, HTTP_CONSUMES]),
    (
        "cross-layer/path-near-miss",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    (
        "cross-layer/route-near-miss",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    ("cross-layer/prefix-drift", &[HTTP_PROVIDES, HTTP_CONSUMES]),
    (
        "cross-layer/db-table-name-in-multiple-sources",
        &[DB_TABLE_PROVIDES, DB_TABLE_CONSUMES],
    ),
    (
        "cross-layer/duplicate-route",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    (
        "cross-layer/external-shadow-internal",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    ("cross-layer/external-secret-in-url", &[HTTP_CONSUMES]),
    (
        "cross-layer/external-host-in-multiple-sources",
        &[HTTP_CONSUMES],
    ),
    ("cross-layer/external-host-fanout", &[HTTP_CONSUMES]),
    ("cross-layer/external-base-url-drift", &[HTTP_CONSUMES]),
    (
        "cross-layer/external-version-inconsistent",
        &[HTTP_CONSUMES],
    ),
    ("cross-layer/external-ip-literal", &[HTTP_CONSUMES]),
    (
        "cross-layer/ambiguous-consume",
        &[HTTP_PROVIDES, HTTP_CONSUMES, TRPC_PROVIDES, TRPC_CONSUMES],
    ),
    (
        "cross-layer/all-consumes-unjoined",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    (
        "cross-layer/unconsumed-mutation-endpoint",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    (
        "cross-layer/unprovided-mutation-call",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    ("cross-layer/route-shadowing", &[HTTP_PROVIDES]),
    ("cross-layer/unresolved-consume-ratio", &[HTTP_CONSUMES]),
    (
        "cross-layer/untraced-client-import-no-visible-consume",
        &[HTTP_CONSUMES],
    ),
    ("cross-layer/unconsumed-procedure", &[TRPC_PROVIDES]),
    (
        "cross-layer/body-field-drift",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    (
        "cross-layer/sensitive-response-field",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    (
        "cross-layer/retrying-write-no-idempotency",
        &[HTTP_PROVIDES, HTTP_CONSUMES],
    ),
    ("cross-layer/unknown-verb-route", &[HTTP_PROVIDES]),
];

/// Registers every native analysis id whose implementation lives in this crate (see `rules/README.md`'s
/// "Adding a rule" section); `zzop_engine::register_all_native` composes this with the other crates' own.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    for row in native_rule_channels() {
        register_native_analysis_stub(registry, &row.rule_id);
    }
}

/// This crate's half of the rule→io-channel declaration, composed with the other crates' own by
/// `zzop_engine::native_rule_channels` — the same aggregator shape as [`register_native_analyses`].
pub fn native_rule_channels() -> Vec<NativeRuleChannels> {
    declare_native_rule_channels(NATIVE_ANALYSES)
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
