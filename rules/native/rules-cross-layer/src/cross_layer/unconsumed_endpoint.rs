//! `cross-layer/unconsumed-endpoint` (info) — one finding per `CrossLayerResult::unconsumed_provides` entry
//! of kind `"http"` that no source in this `analyzeTrees` run calls AND that the write-verb specialization
//! below does not already report. Severity starts at info (not warning) because "no consumer WITHIN this
//! analysis" is weaker evidence than "no consumer at all" — see the message's own caveat.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — a route registered
//! in a test fixture is not deployed surface. A dead route provided by 2+ trees ALSO fires one warning
//! `cross-layer/duplicate-route` finding for the same key — intentional overlap, different questions.
//!
//! ## Routes `unconsumed-mutation-endpoint` already reported are not repeated here
//! A write route IS an endpoint, so `cross-layer/unconsumed-mutation-endpoint` is a strict specialization of
//! this rule: for a while both fired at the identical `file:line` (dogfood measured `POST /api/ledger/{}/verify`
//! reported twice), which is one route billed as two problems. `already_reported_sites` — the
//! `(source, file, line, key)` tuples the specialization ACTUALLY emitted this run, produced by
//! [`unconsumed_mutation_endpoint::reported_provide_sites`] at the `zzop_engine::cross_layer_findings` call
//! site — makes this rule stand down on exactly those ROUTES. Keyed on real output, never on a local copy of
//! the sibling's write-verb predicate: a second copy would drift the moment either rule's exclusions change,
//! re-opening the double report (or worse, a silent gap) without any test noticing.
//!
//! The interface key is part of that tuple on purpose. One `file:line` is routinely several routes — a
//! verb-agnostic registration (gin `router.Any`, axum `any(...)`, the TS pathname-dispatch adapter emitting
//! every verb it scanned at the path test's line) yields one provide per method at a single anchor. Standing
//! down on the ANCHOR would mean an unconsumed `POST /webhook` also silenced the co-located `GET /webhook`,
//! in neither rule: precisely the silent hole this handoff exists to prevent.
//!
//! The resulting contract, which the tests pin from both ends: with the specialization ENABLED a write route
//! is reported exactly once (by it); with it DISABLED nothing is suppressed and this rule reports those write
//! routes itself — turning one rule off must never punch a silent hole in a rule the user did not disable.
//! Per the repo's cross-reference doctrine the handoff is disclosed on both sides: this rule's message names
//! the sibling as the place write routes are reported, and the sibling's message says this rule stands down.
//!
//! ## Externally-fetched paths are vetoed, not reported
//! [`EXTERNALLY_FETCHED_PATHS`] is a small policy vocabulary of paths whose requester is, by construction,
//! outside every analyzed tree: monitors, browsers, crawlers and feed readers. For those, "no consumer in this
//! analysis" is not weak evidence of deadness — it is NO evidence, since the caller could never have been in
//! the analyzed source to begin with (dogfood measured 3 of 4 `unconsumed-endpoint` hits to be exactly this:
//! `GET /health`, `GET /` and `GET /feed.xml`). Deliberately NOT a general "probably external" heuristic —
//! only tokens whose external requester is defined by a protocol or a browser behavior qualify.
//!
//! Which paths those are is a name a PROJECT picks (`/livez` vs `/ping`), so the list is the default behind
//! the declarable `vocabulary.externallyFetchedPaths` and reaches this rule as a parameter. The message
//! renders the EFFECTIVE list rather than a prose copy of the built-in — a declared vocabulary would
//! otherwise make every finding misreport what this run actually vetoed. The RFC 8615 `/.well-known/`
//! prefix stays built in: it is a registry, not a name anyone picks.
//!
//! ## Volume: a per-SOURCE fold, not a filter
//! On a backend-only or framework-package tree EVERY route is unconsumed by construction — the callers
//! were never in the run (dogfood: medusa 503, n8n 479). That list is a census, and printing it line by
//! line drowns every other finding in the report. Past [`MAX_LISTED_PER_SOURCE`] endpoints in one source
//! the tail collapses into ONE finding carrying the remaining count ([`fold::fold_per_source`]). Three
//! properties make this a fold rather than a silent cap: it is per SOURCE (a two-finding frontend beside a
//! 503-finding backend keeps both of its own), the fold finding declares how many routes it stands for and points at
//! `crossLayer.unconsumedProvides` — which is uncapped and unchanged — and it carries the same rule id, so
//! disabling/overriding the rule still means one thing.

mod fold;

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::{TaggedConsume, TaggedProvide};
use zzop_core::{disable_hint, Finding, Severity};

use super::route_near_miss::NearMissTargetRef;
use super::split_key;
use fold::fold_per_source;

/// Exact route paths fetched from outside any analyzable source tree by definition. Each token earns its
/// place from a protocol or a browser/monitor behavior, never from "feels infra":
/// - `/` — the site root every browser, uptime monitor and crawler requests first.
/// - `/health`, `/healthz`, `/healthcheck`, `/livez`, `/readyz` — liveness/readiness probe paths, called by
///   an orchestrator or an uptime monitor (Kubernetes `livenessProbe`/`readinessProbe`, load balancers),
///   never by application code.
/// - `/robots.txt` — Robots Exclusion Protocol (RFC 9309); fetched by crawlers.
/// - `/sitemap.xml`, `/sitemap_index.xml` — the sitemaps.org protocol's fixed entry points; fetched by
///   crawlers.
/// - `/rss.xml`, `/feed.xml`, `/atom.xml` — feed endpoints polled by RSS/Atom readers.
/// - `/favicon.ico` — requested by browsers with no markup reference at all.
///
/// Extension-bearing feed paths ONLY: a bare `/feed` or `/rss` is just as likely an in-app page route, and
/// vetoing it would hide a genuinely dead page. Likewise absent on purpose: `/metrics`, `/status`, `/ping`,
/// `/login`, `/logout`, `/version` — common, but each is also a plausible application resource, so silence
/// there would cost real signal.
pub const EXTERNALLY_FETCHED_PATHS: &[&str] = &[
    "/",
    "/health",
    "/healthz",
    "/healthcheck",
    "/livez",
    "/readyz",
    "/robots.txt",
    "/sitemap.xml",
    "/sitemap_index.xml",
    "/rss.xml",
    "/feed.xml",
    "/atom.xml",
    "/favicon.ico",
];

/// How many unconsumed endpoints one SOURCE lists individually before the tail collapses into a single
/// disclosed fold finding (see the module doc's volume section, and [`fold::fold_per_source`] for the
/// collapse). Per source, not per run: a small tree analyzed alongside a framework package must not lose
/// its own three findings to the other tree's five hundred.
///
/// Deliberately NOT unified with `FANOUT_MIN_FILES`/`MIN_PREFIX_DRIFT_GROUP`/`MIN_FOREIGN_UNPROVIDED_GROUP`
/// (policy inventory T3): those are EVIDENCE floors ("2 is coincidence, 3+ is a pattern") and this is a
/// READABILITY ceiling — opposite direction, different question, free to move alone.
///
/// The value is a judgment with two anchors, both stated rather than implied. Below: the cap must be high
/// enough that a tree whose dead-route list is still something a person reads item by item is completely
/// unaffected — no existing finding disappears, which is why this is not 3 or 10. Above: the measured
/// structural cases this exists for (dogfood: medusa 503, n8n 479 unconsumed endpoints on BE-only /
/// framework-package trees, where EVERY route is unconsumed by construction) sit an order of magnitude
/// higher, so the fold fires there and only there. Nothing is dropped — the folded routes stay in
/// `crossLayer.unconsumedProvides`, uncapped, and the fold finding says so.
const MAX_LISTED_PER_SOURCE: usize = 25;

/// RFC 8615's well-known URI registry: everything under it exists precisely so an external agent (ACME
/// validator, security researcher, OIDC client, mobile OS deep-link verifier) can fetch it at a fixed path.
/// A prefix, not a token list, because the registry is open-ended.
const WELL_KNOWN_PREFIX: &str = "/.well-known/";

/// True when the path is one an external, non-analyzable requester fetches by definition — see
/// [`EXTERNALLY_FETCHED_PATHS`], of which `vetoed` is this run's effective spelling. Compared
/// case-insensitively and with a trailing slash trimmed (`/health/` and `/Health` are the same route),
/// against the whole path, so `/api/health` and `/health-report` — which are ordinary application routes —
/// never match.
fn is_externally_fetched_path(path: &str, vetoed: &[&str]) -> bool {
    let lowered = path.to_ascii_lowercase();
    if lowered.starts_with(WELL_KNOWN_PREFIX) {
        return true;
    }
    let trimmed = lowered.trim_end_matches('/');
    let trimmed = if trimmed.is_empty() { "/" } else { trimmed };
    vetoed.contains(&trimmed)
}

/// True when this provide's key carries an externally-fetched path. Keys that do not split into the
/// `"METHOD /path"` shape are never vetoed — an unrecognized key shape is not evidence of anything.
fn is_externally_fetched_provide(p: &TaggedProvide, vetoed: &[&str]) -> bool {
    split_key(&p.provide.key).is_some_and(|(_, path)| is_externally_fetched_path(path, vetoed))
}

/// `externally_fetched_paths`: the run's declared `vocabulary.externallyFetchedPaths`, else the built-in
/// [`EXTERNALLY_FETCHED_PATHS`]. A declared list REPLACES the built-in whole (the vocabulary contract's
/// per-key whole replacement) and is what the message enumerates, so the finding never advertises a veto
/// this run did not apply.
pub fn unconsumed_endpoint_findings(
    unconsumed_provides: &[TaggedProvide],
    unresolved_consumes: &[TaggedConsume],
    near_miss_targets: &BTreeMap<(String, String, u32), NearMissTargetRef>,
    trpc_participating_sources: &BTreeSet<String>,
    already_reported_sites: &BTreeSet<(String, String, u32, String)>,
    externally_fetched_paths: &[&str],
) -> Vec<Finding> {
    let unresolved_http = unresolved_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
        .count();
    let vetoed_list = externally_fetched_paths
        .iter()
        .map(|p| format!("`{p}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let mut out: Vec<(String, Finding)> = unconsumed_provides
        .iter()
        .filter(|p| p.provide.kind == "http" && !zzop_core::is_test_file(&p.provide.file))
        .filter(|p| {
            !(trpc_participating_sources.contains(&p.source)
                && super::is_trpc_mount_route_key(&p.provide.key))
        })
        // What the specialization really emitted this run — never a local re-derivation of its predicate,
        // and empty when it is disabled, so this rule then covers those routes itself (see the module doc).
        // Matched per ROUTE (the key is in the tuple), not per anchor: one line can register several verbs.
        .filter(|p| {
            !already_reported_sites.contains(&(
                p.source.clone(),
                p.provide.file.clone(),
                p.provide.line,
                p.provide.key.clone(),
            ))
        })
        .filter(|p| !is_externally_fetched_provide(p, externally_fetched_paths))
        .map(|p| {
            let key = &p.provide.key;
            let near_miss = near_miss_targets.get(&(
                p.source.clone(),
                p.provide.file.clone(),
                p.provide.line,
            ));
            let near_miss_note = if let Some(t) = near_miss {
                format!(
                    " However, {} unmatched http consume(s) in this run name this route as their closest \
                     near-miss candidate (see the `cross-layer/route-near-miss` finding at {}:{}) — the route \
                     may actually be called through a drifted or base-relative path rather than being dead.",
                    t.count, t.consume_file, t.consume_line
                )
            } else {
                String::new()
            };
            let message = format!(
                "endpoint `{key}` (source `{}`) is not called by any source in this analysis. This may be \
                 genuinely dead route code, or it may be consumed by a caller this analysis cannot see — a \
                 repo not included in this `analyzeTrees` run, a mobile/native/third-party client, or one of \
                 the {unresolved_http} unresolved dynamic-URL http consume(s) this run could not statically \
                 match to a key (see `crossLayer.unresolvedConsumes`). Confirm with real traffic/access logs before \
                 removing the route.{near_miss_note} Two exclusions apply here: a write route already reported \
                 by `cross-layer/unconsumed-mutation-endpoint` (this rule's write-verb specialization) is not \
                 repeated, so one route is never billed twice — disable that rule and such routes appear here \
                 instead; and the paths fetched by external agents by definition ({vetoed_list}, and anything \
                 under `/.well-known/`) are never reported at all. {} if provider-only endpoints (webhook \
                 targets, endpoints consumed only outside this analysis) are expected in your stack.",
                p.source,
                disable_hint("cross-layer/unconsumed-endpoint")
            );
            let mut data = serde_json::json!({
                "key": key,
                "source": p.source,
                "unresolvedHttpConsumeCount": unresolved_http,
            });
            if let Some(t) = near_miss {
                data["nearMissConsumeCount"] = serde_json::json!(t.count);
                data["nearMissConsumeExample"] =
                    serde_json::json!(format!("{}:{}", t.consume_file, t.consume_line));
            }
            (
                p.source.clone(),
                Finding {
                    rule_id: "cross-layer/unconsumed-endpoint".to_string(),
                    severity: Severity::Info,
                    file: p.provide.file.clone(),
                    line: p.provide.line,
                    message,
                    data: Some(data),
                },
            )
        })
        .collect();
    // Sorted BEFORE the fold so which endpoints get listed is decided by the same (file, line) order the
    // output is read in — not by `unconsumed_provides`' arrival order.
    out.sort_by(|(sa, a), (sb, b)| {
        sa.cmp(sb)
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });
    let mut out = fold_per_source(out, unresolved_http);
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests;
