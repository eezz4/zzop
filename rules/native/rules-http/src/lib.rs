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

use zzop_core::{register_native_analysis_stub, RuleRegistry, Severity};

/// Registers every native analysis id whose implementation lives in this crate (see `rules/README.md`'s
/// "Adding a rule" section); `zzop_engine::register_all_native` composes this with the other crates' own.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    let analyses: &[(&str, Severity)] = &[
        ("duplicate-route", Severity::Warning),
        ("unsafe-read-endpoint", Severity::Warning),
        ("non-idempotent-write", Severity::Warning),
        ("route-shadowing", Severity::Warning),
        ("mutating-route-no-auth", Severity::Info),
        ("unprovided-consume", Severity::Info),
    ];
    for &(id, default_severity) in analyses {
        register_native_analysis_stub(registry, id, default_severity);
    }
}

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
