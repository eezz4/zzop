//! `cross-layer/unconsumed-endpoint` (info) — one finding per `CrossLayerResult::unconsumed_provides` entry of
//! kind `"http"`: an endpoint no source in this `analyzeTrees` run calls. Severity starts at info (not
//! warning) because "no consumer WITHIN this analysis" is weaker evidence than "no consumer at all" — see
//! the message's own caveat.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — a route registered
//! in a test fixture is not deployed surface. A dead route provided by 2+ trees ALSO fires one warning
//! `cross-layer/duplicate-route` finding for the same key — intentional overlap, different questions.
//!
//! ## Near-miss cross-reference
//! When a provide here is ALSO the chosen near-miss target of an unmatched `cross-layer/route-near-miss`
//! consume (`near_miss_targets`, sourced from `route_near_miss::route_near_miss_results`), the message gains
//! a cross-reference note: dogfood round 8 found this to be the common case, not the exception — a
//! disconnected FE/BE pair with a drifted base prefix produces one `unconsumed-endpoint` finding PER route
//! plus one `route-near-miss` finding per drifted consume, describing the same underlying drift from two
//! sides without ever pointing at each other.
//!
//! ## tRPC mount-route suppression
//! A provide [`super::is_trpc_mount_route_key`] identifies as a tRPC mount route (a literal `trpc` path
//! segment, e.g. `/api/trpc/{}`) is excluded here when ITS OWN source tree is in `trpc_participating_sources`
//! (a tree with 1+ `trpc`-kind edge on either side): dogfood round 9 found a fully-joined tRPC starter's
//! only findings were its own GET/POST mount routes — the mount route IS the transport the `trpc`-kind
//! edges flow through, so "unconsumed" is tone noise, not signal. Per-tree, not run-global: a route in a
//! tree with zero tRPC edges of its own is never suppressed, even when some OTHER tree in the run has
//! tRPC edges. The suppression is never silent — `super::trpc_mount_route_suppression_notes` (called by
//! `zzop_engine::analyze_trees`) discloses it on the owning tree's `warnings` channel instead.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::{TaggedConsume, TaggedProvide};
use zzop_core::{disable_hint, Finding, Severity};

use super::route_near_miss::NearMissTargetRef;

pub fn unconsumed_endpoint_findings(
    unconsumed_provides: &[TaggedProvide],
    unresolved_consumes: &[TaggedConsume],
    near_miss_targets: &BTreeMap<(String, String, u32), NearMissTargetRef>,
    trpc_participating_sources: &BTreeSet<String>,
) -> Vec<Finding> {
    let unresolved_http = unresolved_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
        .count();

    let mut out: Vec<Finding> = unconsumed_provides
        .iter()
        .filter(|p| p.provide.kind == "http" && !zzop_core::is_test_file(&p.provide.file))
        .filter(|p| {
            !(trpc_participating_sources.contains(&p.source)
                && super::is_trpc_mount_route_key(&p.provide.key))
        })
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
                 removing the route.{near_miss_note} {} if provider-only endpoints (webhook targets, health probes, \
                 endpoints consumed only outside this analysis) are expected in your stack.",
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
            Finding {
                rule_id: "cross-layer/unconsumed-endpoint".to_string(),
                severity: Severity::Info,
                file: p.provide.file.clone(),
                line: p.provide.line,
                message,
                data: Some(data),
            }
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests;
