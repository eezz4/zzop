//! zzop-rules-http — native whole-graph rules that operate over a repo's SINGLE-TREE HTTP/route
//! surface, git-free.
//!
//! ## Module map
//! - [`http_scan`]: call-graph-BFS HTTP scanners (`scan_unsafe_read_endpoint`, `scan_non_idempotent_write`).
//! - [`duplicate_route`]: same HTTP route registered 2+ times.
//! - [`route_shadowing`]: an earlier param route shadows a later literal route in the same file.
//! - [`mutating_route_no_auth`]: a mutating route's handler never reaches an auth-guard callee via BFS.
//! - [`unprovided_consume`]: a resolved `http` consume with no matching provide anywhere in the analysis.
//!
//! The multi-tree cross-layer join counterpart to these rules lives in `zzop-rules-cross-layer`.
//!
//! Every rule body here depends on `zzop-core` only.

pub mod duplicate_route;
pub mod http_scan;
pub mod mutating_route_no_auth;
pub mod route_shadowing;
pub mod unprovided_consume;

use zzop_core::rule_channels::reads::{HTTP_CONSUMES, HTTP_PROVIDES};
use zzop_core::{
    declare_native_rule_channels, register_native_analysis_stub, NativeRuleChannels, RuleIoChannel,
    RuleRegistry,
};

/// Every native analysis id this crate owns, each on one row with the cross-layer io channels its rule
/// body reads (`zzop_core::rule_channels`' mechanism doc holds the contract). ONE table, two readers —
/// [`register_native_analyses`] and [`native_rule_channels`] — so an id cannot be registered without a
/// channel statement, and the statement cannot drift from the id it describes.
///
/// `unsafe-read-endpoint`/`non-idempotent-write` are the two rows whose channel is not visible in this
/// crate at all: they take `zzop_core::ApiEndpoint`, which the ENGINE reconstructs from the very same
/// http provides (`analyze::native_rules::callgraph`'s `io_provides.iter().filter(|p| p.kind == "http")`)
/// rather than from a route pass of its own. Both are pinned by name in `rule_contracts::rule_channels`
/// against that pre-filter, never skipped.
const NATIVE_ANALYSES: &[(&str, &[RuleIoChannel])] = &[
    ("duplicate-route", &[HTTP_PROVIDES]),
    ("unsafe-read-endpoint", &[HTTP_PROVIDES]),
    ("non-idempotent-write", &[HTTP_PROVIDES]),
    ("route-shadowing", &[HTTP_PROVIDES]),
    ("mutating-route-no-auth", &[HTTP_PROVIDES]),
    ("unprovided-consume", &[HTTP_PROVIDES, HTTP_CONSUMES]),
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

pub use duplicate_route::duplicate_route_findings;
pub use http_scan::{
    rule_sightlines, scan_non_idempotent_write, scan_unsafe_read_endpoint,
    ScanNonIdempotentWriteInput, ScanUnsafeReadEndpointInput, WRITE_HTTP_METHODS,
};
// The convention-vocabulary constants are exported so the ONE crate that assembles declarable defaults
// (`zzop_engine::VocabularyConfig::built_in`) references these symbols instead of copying their values —
// a second copy of a guard vocabulary is a second answer to "what does zzop think a guard is called".
pub use mutating_route_no_auth::qualifier::QUALIFIER_GUARD_TOKENS;
pub use mutating_route_no_auth::{
    scan_mutating_route_no_auth, ScanMutatingRouteNoAuthInput,
    AUTH_ACQUISITION_CONDITIONAL_PATTERN, AUTH_ACQUISITION_STANDALONE_PATTERN,
    AUTH_FAMILY_PATH_PATTERN, CALL_GRAPH_COVERED_EXTENSIONS, DEFAULT_AUTH_GUARD_PATTERN,
};
pub use route_shadowing::route_shadowing_findings;
pub use unprovided_consume::{unprovided_consume_findings, API_SEGMENT_PATTERN};
