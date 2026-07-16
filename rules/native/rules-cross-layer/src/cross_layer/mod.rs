//! `cross-layer/*` — 23 native rules that run over `zzop_core::CrossLayerResult`, the multi-tree join result
//! `zzop_engine::analyze_trees` produces (see `crates/core/src/io.rs`'s module doc for the join itself:
//! exact `(kind, key)` join with an ambiguity gate for keys provided by 2+ distinct source trees, an
//! external-egress gate for host-carrying consume keys, and a low-confidence tag for generic paths).
//! Every rule here is a pure function over `&CrossLayerResult` (+ the provide-key universe some of them
//! need — see [`HttpProvideSite`]), never touching a single tree's `CommonIr` directly: the whole point of
//! this module is joint-graph reasoning that no single-tree native rule (`duplicate_route`,
//! `unprovided_consume`, ...) can see.
//!
//! ## Module map
//! - [`unconsumed_endpoint`]: `unconsumed_endpoint_findings` — an HTTP endpoint no analyzed tree calls
//!   (`cross-layer/unconsumed-endpoint`, info).
//! - [`method_mismatch`]: `method_mismatch_findings` — an unprovided consume whose path matches a provide
//!   exactly but the method differs (`cross-layer/method-mismatch`, warning).
//! - [`version_skew`]: `version_skew_findings` — an unprovided consume whose key differs from a provide only in
//!   a version path segment (`cross-layer/version-skew`, warning).
//! - [`path_near_miss`]: `path_near_miss_findings` — an unprovided consume whose key matches a provide after
//!   allowing `{}` positions to differ, but is otherwise segment-identical (`cross-layer/path-near-miss`,
//!   info).
//! - [`route_near_miss`]: `route_near_miss_findings` — an unprovided consume whose key differs from a
//!   same-method provide by exactly one of `case`/`prefix` (an all-literal 1-2 segment base path),
//!   disjoint from `path_near_miss`'s parameter-generalization case (`cross-layer/route-near-miss`, info).
//! - [`prefix_drift`]: `prefix_drift_findings` — a pure aggregation over `route_near_miss`'s prefix records:
//!   when 3+ consumes from one tree all near-miss providers in another tree by the SAME missing/extra path
//!   prefix, emits ONE `cross-layer/prefix-drift` (info) naming the single base-path cause instead of N
//!   per-route near-misses. The engine call site suppresses the subsumed per-route `route-near-miss`
//!   findings (`retain_non_subsumed`) — a replacement, not silent suppression: the aggregate enumerates
//!   every folded route. Structurally derived, so it only fires when `route-near-miss` is enabled.
//! - [`shared_db_table`]: `shared_db_table_findings` — the same `db-table` key consumed by 2+ distinct
//!   source trees (`cross-layer/shared-db-table`, warning).
//! - [`duplicate_route`]: `cross_layer_duplicate_route_findings` — the same `http` `(method, path)` key
//!   PROVIDED by 2+ distinct source trees (`cross-layer/duplicate-route`, warning) — distinct from the
//!   existing single-tree `zzop_rules_http::duplicate_route` rule (different id, different join scope: this one only
//!   fires across trees, never within one).
//!
//! The `external_consumes` bucket (host-carrying consume keys, verbatim URLs) has its own dedicated readers;
//! the first seven rules below are those.
//! - [`external_shadow_internal`]: an external consume whose normalized method+path matches a route an
//!   analyzed tree provides — the caller hardcodes an environment host instead of the relative/proxy path
//!   (`cross-layer/external-shadow-internal`, warning).
//! - [`external_secret_in_url`]: a secret-named query parameter in an external URL
//!   (`cross-layer/external-secret-in-url`, warning).
//! - [`external_duplicated_integration`]: the same external host called directly from 2+ distinct trees
//!   (`cross-layer/external-duplicated-integration`, warning).
//! - [`external_host_fanout`]: the same external host called directly from 3+ distinct files
//!   (`cross-layer/external-host-fanout`, info).
//! - [`external_base_url_drift`]: the same external path consumed against 2+ different hosts
//!   (`cross-layer/external-base-url-drift`, info).
//! - [`external_version_inconsistent`]: one host consumed through both version-shaped and versionless paths
//!   (`cross-layer/external-version-inconsistent`, info).
//! - [`external_ip_literal`]: an external host that is a raw IP literal, loopback excluded
//!   (`cross-layer/external-ip-literal`, warning).
//! - [`ambiguous_consume`]: a consume whose key 2+ distinct trees provide — deploy-time routing decides
//!   (`cross-layer/ambiguous-consume`, warning).
//! - [`unconsumed_mutation_endpoint`]: an unconsumed provide with a write method — standing attack surface
//!   (`cross-layer/unconsumed-mutation-endpoint`, warning; downgraded to info, with a named-source
//!   explanation, when the run has 1+ [`majority_unresolved_http_sources`] BLIND source — a confident
//!   "unconsumed" verdict requires a resolved consume side; co-fires with `unconsumed-endpoint` by design).
//! - [`unprovided_mutation_call`]: an unprovided consume with a write method — a state-changing call going nowhere
//!   visible (`cross-layer/unprovided-mutation-call`, warning; co-fires with the unprovided-diagnosis trio).
//! - [`cross_tree_route_shadowing`]: a `{}`-pattern route in one tree that would shadow a same-method
//!   literal route provided by a DIFFERENT tree behind a shared first-match gateway
//!   (`cross-layer/route-shadowing`, warning — distinct id scope from the single-tree, single-file
//!   `route-shadowing`).
//! - [`unresolved_consume_ratio`]: a tree whose http consumes are majority-unresolved — self-reports that
//!   the join is mostly blind for that tree (SDK/wrapper/dynamic-URL indirection) instead of staying silent
//!   (`cross-layer/unresolved-consume-ratio`, info).
//! - [`sdk_import_no_visible_consume`]: a tree importing an SDK-shaped package from several files while
//!   having fewer visible http consumes than even `unresolved_consume_ratio`'s floor — the
//!   not-even-visible half of the blind-spot partition (`cross-layer/sdk-import-no-visible-consume`, info).
//!   A tree that calls its API entirely through a generated SDK client can show zero visible http
//!   consumes, leaving every consume-ratio-based blind-spot rule silent — this rule catches that case.
//! - [`unconsumed_procedure`] (kind="trpc"): a tRPC procedure (composed by the engine from
//!   router fragments, key `"VERB dotted.path"`) that no analyzed tree calls — the compiler catches calls
//!   to nonexistent procedures but not unused definitions (`cross-layer/unconsumed-procedure`, info).
//! - [`body_field_drift`]: `body_field_drift_findings` — a matched `http` edge whose FE-witnessed request-
//!   body literal (`body-shape-v1`'s `ConsumeBodyShape`) disagrees with the BE handler's resolved DTO
//!   (`ProvideBodyShape`): a missing required field, an undeclared extra key (only when the DTO's field
//!   list is complete), or a missing `@Body('subKey')` wrapper (`cross-layer/body-field-drift`, warning).
//!
//! ## Suppression
//! None of these rules honor an inline `// <marker>-ok` suppression comment. Checked against how the
//! existing native rules in this crate do it: `duplicate_route`/`route_shadowing`/`unprovided_consume`
//! carry no marker support either, and `mutating_route_no_auth`'s own message says so explicitly ("this
//! rule has no inline suppression marker") — inline markers are a DSL-only mechanism
//! (`zzop_core::dsl::RuleDef::suppress_marker`), never wired into any native rule's `Finding` construction.
//! Every rule here is disable-only via `RuleConfig::disabled_rules` (message text says so).
//!
//! ## The provide-key universe
//! `method_mismatch`/`version_skew`/`path_near_miss`/`route_near_miss`/`external_shadow_internal`/
//! `cross_tree_route_shadowing` need to compare against every `http` provide across every tree, not just the
//! ones `CrossLayerResult`
//! happens to expose (`unconsumed_provides` excludes ambiguous-candidate provides; `edges`/`ambiguous_consumes` only cover
//! provides some consume already matched). That full universe is deliberately NOT threaded through
//! `zzop_core::io::link_cross_layer_io`'s return type (`crates/core` stays rule-vocabulary-free by design —
//! the kernel carries mechanisms, never rule data); instead the engine call site
//! (`zzop_engine::analyze_trees`) derives a flat `Vec<HttpProvideSite>` straight from the same `SourceIo`
//! inputs it already built for the join, and passes it into these rule functions directly. See
//! [`HttpProvideSite`]'s own doc. The same reasoning covers `unresolved_consume_ratio`'s per-tree http
//! consume totals (`Vec<(String, usize)>`, engine-derived).
//!
//! One unprovided consume CAN legitimately fire 2+ of `method_mismatch`/`version_skew`/`path_near_miss`/
//! `route_near_miss`/`unprovided_mutation_call` at once when different comparisons hold (e.g. consume `POST /api/v1/orders` against
//! provides `PUT /api/v1/orders` and `POST /api/v2/orders`). That co-firing is intentional, not a dedup
//! bug — each finding carries a distinct diagnosis of the same broken call. Likewise `unconsumed_mutation_endpoint`
//! intentionally co-fires with `unconsumed_endpoint` (same site, severity-split diagnosis).

pub mod ambiguous_consume;
pub mod body_field_drift;
pub mod cross_tree_route_shadowing;
pub mod duplicate_route;
pub mod external_base_url_drift;
pub mod external_duplicated_integration;
pub mod external_host_fanout;
pub mod external_ip_literal;
pub mod external_secret_in_url;
pub mod external_shadow_internal;
pub mod external_version_inconsistent;
pub mod method_mismatch;
pub mod path_near_miss;
pub mod prefix_drift;
pub mod route_near_miss;
pub mod sdk_import_no_visible_consume;
pub mod shared_db_table;
pub mod unconsumed_endpoint;
pub mod unconsumed_mutation_endpoint;
pub mod unconsumed_procedure;
pub mod unprovided_mutation_call;
pub mod unresolved_consume_ratio;
pub mod version_skew;

// Root-file helpers moved out purely for file-size layout, re-exported below so every existing
// `super::`/crate-root path still resolves (see each module's own doc).
mod external_url;
mod trpc_mount;

pub use ambiguous_consume::ambiguous_consume_findings;
pub use body_field_drift::body_field_drift_findings;
pub use cross_tree_route_shadowing::cross_tree_route_shadowing_findings;
pub use duplicate_route::cross_layer_duplicate_route_findings;
pub use external_base_url_drift::external_base_url_drift_findings;
pub use external_duplicated_integration::external_duplicated_integration_findings;
pub use external_host_fanout::external_host_fanout_findings;
pub use external_ip_literal::external_ip_literal_findings;
pub use external_secret_in_url::external_secret_in_url_findings;
pub use external_shadow_internal::external_shadow_internal_findings;
pub use external_version_inconsistent::external_version_inconsistent_findings;
pub use method_mismatch::method_mismatch_findings;
pub use path_near_miss::path_near_miss_findings;
pub use prefix_drift::{prefix_drift_findings, retain_non_subsumed, PrefixDriftOutput};
pub use route_near_miss::{
    route_near_miss_findings, route_near_miss_results, NearMissTargetRef, RouteNearMissOutput,
};
pub use sdk_import_no_visible_consume::sdk_import_no_visible_consume_findings;
pub use shared_db_table::shared_db_table_findings;
pub use unconsumed_endpoint::unconsumed_endpoint_findings;
pub use unconsumed_mutation_endpoint::unconsumed_mutation_endpoint_findings;
pub use unconsumed_procedure::unconsumed_procedure_findings;
pub use unprovided_mutation_call::unprovided_mutation_call_findings;
pub use unresolved_consume_ratio::unresolved_consume_ratio_findings;
pub use version_skew::version_skew_findings;

pub(crate) use external_url::split_external_key;
pub(crate) use trpc_mount::is_trpc_mount_route_key;
pub use trpc_mount::trpc_mount_route_suppression_notes;

/// One `http` provide site, tagged with its source tree — the flat "provide-key universe" `method_mismatch`/
/// `version_skew`/`path_near_miss` need (see module doc). Deliberately a plain local struct, not a reuse of
/// `zzop_core::io::TaggedProvide`: the caller (`zzop_engine::analyze_trees`) already has exactly this shape in
/// hand from its own `SourceIo` list and this crate depends on `zzop-core` only for its actual IR/Finding
/// contracts, not as a place to borrow one more struct shape from for a purely-local aggregation.
#[derive(Debug, Clone)]
pub struct HttpProvideSite {
    pub source: String,
    /// The full normalized `"METHOD /path"` key (`zzop_core::http_interface_key`'s output shape).
    pub key: String,
    pub file: String,
    pub line: u32,
}

/// Splits a normalized `"METHOD /path"` key into `(method, path)`, or `None` if it doesn't carry the
/// expected space-separated shape (defensive — every real `http_interface_key` output does).
pub(crate) fn split_key(key: &str) -> Option<(&str, &str)> {
    key.split_once(' ')
}

/// Splits a path into its non-empty `/`-delimited segments (`"/a/{}/b"` -> `["a", "{}", "b"]`).
pub(crate) fn path_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// Consume-side minimum-information gate for the near-miss rules.
///
/// True when every segment is the opaque `{}` placeholder (or the path has no
/// segments at all). Such keys are typically head-drop artifacts (e.g.
/// `` `${base}/${x}` `` keying as `GET /{}`): they carry zero literal evidence,
/// so a near-miss suggestion computed from them is vacuous — an all-slot key
/// "resembles" every same-length route. Deliberately asymmetric: only consume
/// keys are gated. An all-slot *provide* is a declared catch-all route (e.g.
/// `app.get('/:page')`) and remains a legitimate suggestion target.
pub(crate) fn is_all_slot_path(segments: &[&str]) -> bool {
    segments.iter().all(|s| *s == "{}")
}

/// A version-shaped path segment: `v1`, `V2`, `v1.2`, ... — shared by `version_skew` (dangling-vs-provide
/// skew) and `external_version_inconsistent` (versioned/versionless mix against one external host).
pub(crate) const VERSION_SEGMENT_PATTERN: &str = r"(?i)^v[0-9]+(?:\.[0-9]+)*$";

/// Trees below this many total `http` consumes are too small for a ratio claim — shared floor between
/// `unresolved_consume_ratio` (fires at/above it) and `sdk_import_no_visible_consume` (fires below it),
/// so the two blind-spot self-reports partition the space and never co-fire on one tree. Also the floor
/// [`majority_unresolved_http_sources`] uses to decide which sources are eligible to count as BLIND at all.
pub(crate) const MIN_TOTAL_CONSUMES: usize = 5;

/// Majority threshold, integer math only (no floats — output must be byte-stable across platforms):
/// `unresolved * 2 >= total` is equivalent to `unresolved / total >= 0.5` without any floating-point
/// division. Single definition, shared by [`majority_unresolved_http_sources`] and `unresolved_consume_ratio`
/// so the two can never drift apart on what "majority" means.
pub(crate) fn is_majority_unresolved(unresolved: usize, total: usize) -> bool {
    unresolved * 2 >= total
}

/// Sources whose `http` consumes are majority-unresolved (key extraction failed for most call sites) AND
/// above the small-sample floor ([`MIN_TOTAL_CONSUMES`]) — i.e. sources the cross-layer join is effectively
/// BLIND to. Single definition shared by `unresolved-consume-ratio` (which discloses the blindness per
/// source) and the `unconsumed-*` rules (which must not over-claim a confident "unconsumed" verdict when a
/// blind source could be the unseen caller). Integer math only — no floats reach output.
///
/// Field defect (mono-hub review, first external v0.14.0 reviews): `cross-layer/unconsumed-mutation-endpoint`
/// fired Warning on write routes that WERE actually called, just through URLs this run's egress extraction
/// couldn't resolve (83% of one tree's http consumes, in the field case) — the highest-severity finding was
/// the least trustworthy one. This helper is the shared predicate that lets both the disclosure rule
/// (`unresolved_consume_ratio`) and the confidence-gated rules reason about blindness identically, so they
/// can never silently drift apart on the definition again.
pub fn majority_unresolved_http_sources(
    unresolved_consumes: &[zzop_core::io::TaggedConsume],
    http_consume_totals: &[(String, usize)],
) -> std::collections::BTreeSet<String> {
    let mut unresolved_by_source: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for c in unresolved_consumes {
        if c.consume.kind == "http" {
            *unresolved_by_source.entry(c.source.as_str()).or_insert(0) += 1;
        }
    }

    http_consume_totals
        .iter()
        .filter_map(|(source, total)| {
            let total = *total;
            if total < MIN_TOTAL_CONSUMES {
                return None;
            }
            let unresolved_count = *unresolved_by_source.get(source.as_str())?;
            is_majority_unresolved(unresolved_count, total).then(|| source.clone())
        })
        .collect()
}

/// One non-relative (package) import specifier observed in a tree, aggregated per specifier — the input
/// `sdk_import_no_visible_consume` needs. Engine-derived like [`HttpProvideSite`] and for the same reason:
/// the kernel's tree IR deliberately drops package imports during dep resolution, but the engine's
/// assembly pass has every file's `ImportMap` in hand and summarizes non-relative specifiers cheaply
/// (mechanism, not rule vocabulary — the SDK-shape filtering happens inside the rule).
#[derive(Debug, Clone)]
pub struct PackageImportSite {
    pub source: String,
    /// Verbatim non-relative specifier (`@vendor/sdk`, `react`, `lodash/get`).
    pub specifier: String,
    /// Number of distinct files in the tree importing this specifier.
    pub file_count: usize,
    /// Lexicographically first importing file — the finding anchor.
    pub example_file: String,
}

/// The HTTP methods that mutate state. Deliberately the write-verb list only — HEAD/OPTIONS never appear in
/// egress extraction (only the 5 verbs `parser-typescript/src/egress.rs` recognizes reach a consume key).
pub(crate) fn is_write_method(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_all_slot_path_pins_the_gate_shape() {
        assert!(is_all_slot_path(&["{}"]));
        assert!(is_all_slot_path(&["{}", "{}"]));
        assert!(is_all_slot_path(&[]));
        assert!(!is_all_slot_path(&["users", "{}"]));
    }
}
