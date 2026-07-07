//! `cross-layer/*` — 20 native rules that run over `zzop_core::CrossLayerResult`, the multi-tree join result
//! `zzop_engine::analyze_trees` produces (see `packages/core/src/io.rs`'s module doc for the join itself:
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
//!   (`cross-layer/unconsumed-mutation-endpoint`, warning; co-fires with `unconsumed-endpoint` by design).
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
//!
//! ## Suppression
//! None of these rules honor an inline `// <marker>-ok` suppression comment. Checked against how the
//! existing native rules in this crate do it: `duplicate_route`/`route_shadowing`/`unprovided_consume`
//! carry no marker support either, and `mutating_route_no_auth`'s own message says so explicitly ("native
//! rules have no inline suppression marker") — inline markers are a DSL-only mechanism
//! (`zzop_core::dsl::RuleDef::suppress_marker`), never wired into any native rule's `Finding` construction.
//! Every rule here is disable-only via `RuleConfig::disabled_rules` (message text says so).
//!
//! ## The provide-key universe
//! `method_mismatch`/`version_skew`/`path_near_miss`/`external_shadow_internal`/`cross_tree_route_shadowing`
//! need to compare against every `http` provide across every tree, not just the ones `CrossLayerResult`
//! happens to expose (`unconsumed_provides` excludes ambiguous-candidate provides; `edges`/`ambiguous_consumes` only cover
//! provides some consume already matched). That full universe is deliberately NOT threaded through
//! `zzop_core::io::link_cross_layer_io`'s return type (`packages/core` stays rule-vocabulary-free by design —
//! the kernel carries mechanisms, never rule data); instead the engine call site
//! (`zzop_engine::analyze_trees`) derives a flat `Vec<HttpProvideSite>` straight from the same `SourceIo`
//! inputs it already built for the join, and passes it into these rule functions directly. See
//! [`HttpProvideSite`]'s own doc. The same reasoning covers `unresolved_consume_ratio`'s per-tree http
//! consume totals (`Vec<(String, usize)>`, engine-derived).
//!
//! One unprovided consume CAN legitimately fire 2+ of `method_mismatch`/`version_skew`/`path_near_miss`/
//! `unprovided_mutation_call` at once when different comparisons hold (e.g. consume `POST /api/v1/orders` against
//! provides `PUT /api/v1/orders` and `POST /api/v2/orders`). That co-firing is intentional, not a dedup
//! bug — each finding carries a distinct diagnosis of the same broken call. Likewise `unconsumed_mutation_endpoint`
//! intentionally co-fires with `unconsumed_endpoint` (same site, severity-split diagnosis).

pub mod ambiguous_consume;
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
pub mod sdk_import_no_visible_consume;
pub mod shared_db_table;
pub mod unconsumed_endpoint;
pub mod unconsumed_mutation_endpoint;
pub mod unconsumed_procedure;
pub mod unprovided_mutation_call;
pub mod unresolved_consume_ratio;
pub mod version_skew;

pub use ambiguous_consume::ambiguous_consume_findings;
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
pub use sdk_import_no_visible_consume::sdk_import_no_visible_consume_findings;
pub use shared_db_table::shared_db_table_findings;
pub use unconsumed_endpoint::unconsumed_endpoint_findings;
pub use unconsumed_mutation_endpoint::unconsumed_mutation_endpoint_findings;
pub use unconsumed_procedure::unconsumed_procedure_findings;
pub use unprovided_mutation_call::unprovided_mutation_call_findings;
pub use unresolved_consume_ratio::unresolved_consume_ratio_findings;
pub use version_skew::version_skew_findings;

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

/// A version-shaped path segment: `v1`, `V2`, `v1.2`, ... — shared by `version_skew` (dangling-vs-provide
/// skew) and `external_version_inconsistent` (versioned/versionless mix against one external host).
pub(crate) const VERSION_SEGMENT_PATTERN: &str = r"(?i)^v[0-9]+(?:\.[0-9]+)*$";

/// Trees below this many total `http` consumes are too small for a ratio claim — shared floor between
/// `unresolved_consume_ratio` (fires at/above it) and `sdk_import_no_visible_consume` (fires below it),
/// so the two blind-spot self-reports partition the space and never co-fire on one tree.
pub(crate) const MIN_TOTAL_CONSUMES: usize = 5;

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

/// An `external_consumes`-bucket consume key decomposed: `"GET https://api.vendor.com:8443/v1/users?key=x"` ->
/// method `GET`, host `api.vendor.com:8443` (port kept — port drift IS config drift), path `/v1/users`
/// (leading slash kept; `"/"` when the URL has no path), query `Some("key=x")`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExternalUrl<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub query: Option<&'a str>,
}

/// Splits an external consume key (`"METHOD scheme://host[/path][?query]"`) into its parts, or `None` when
/// the key doesn't carry the host-marker shape the join's external gate keys on (defensive — every key the
/// `external_consumes` bucket holds does contain `"://"`).
pub(crate) fn split_external_key(key: &str) -> Option<ExternalUrl<'_>> {
    let (method, url) = key.split_once(' ')?;
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    let (host, path_query) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if host.is_empty() {
        return None;
    }
    let (path, query) = match path_query.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path_query, None),
    };
    Some(ExternalUrl {
        method,
        host,
        path,
        query,
    })
}
