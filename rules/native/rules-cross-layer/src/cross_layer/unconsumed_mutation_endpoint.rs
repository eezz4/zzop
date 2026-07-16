//! `cross-layer/unconsumed-mutation-endpoint` (warning, downgraded to info when the run has a blind consume
//! side) — one finding per unconsumed write-verb HTTP provide (`is_write_method`: POST/PUT/PATCH/DELETE): an
//! endpoint that MUTATES state and that no source in this analysis calls. An unconsumed write endpoint is
//! standing attack surface — reachable by anyone who finds it — not merely dead code, hence a warning here
//! versus the plain info of `cross-layer/unconsumed-endpoint`. This rule intentionally co-fires with that
//! rule for the same site: it reports "unreferenced" uniformly across all methods, while this one is the
//! severity-split for the write subset specifically.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — not deployed surface.
//!
//! ## Confidence downgrade when the run is blind
//! Field defect (mono-hub review, first external v0.14.0 reviews): this rule fired Warning unconditionally,
//! even on a run whose own consume side was mostly unresolved (83% of one tree's `http` consumes, in the
//! field case) — the highest-severity cross-layer finding turned out to be the least trustworthy, because a
//! zero ("unconsumed") is only a confident zero when the consume key space was actually resolved
//! (`output-philosophy.md` §1). When `blind_sources` (`super::majority_unresolved_http_sources`, the same
//! predicate `unresolved_consume_ratio` uses to self-report) is non-empty for this run, "unconsumed" cannot
//! be trusted as a confident zero — this rule de-escalates to `Severity::Info` and names the blind source(s)
//! in the message instead of silently keeping Warning. With zero blind sources, severity and message keep
//! today's Warning/"standing attack surface" framing unchanged. This is a de-escalation to match confidence,
//! NOT suppression — the finding still fires either way (`output-philosophy.md` §0: total by default).
//!
//! ## Near-miss cross-reference
//! Same annotation as the sibling `unconsumed_endpoint`: when a write provide here is ALSO the chosen
//! near-miss target of an unmatched `cross-layer/route-near-miss` consume (`near_miss_targets`, sourced from
//! `route_near_miss::route_near_miss_results`), the message gains a cross-reference note pointing at that
//! finding — see `unconsumed_endpoint`'s module doc for the dogfood motivation.
//!
//! ## tRPC mount-route suppression
//! Same exclusion as the sibling `unconsumed_endpoint` (see its module doc): a provide
//! [`super::is_trpc_mount_route_key`] identifies as a tRPC mount route is excluded here too when ITS OWN
//! source tree is in `trpc_participating_sources` — a POST-verb tRPC mount (`file_routes`'s
//! `pages/api/**` fallback-verb convention emits both GET and POST for a default-export handler) would
//! otherwise ALSO fire this write-verb rule for the exact same tone-noise site. Per-tree, not run-global:
//! see `unconsumed_endpoint`'s module doc for why a run-global edge count would misattribute suppression
//! across trees.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::{TaggedConsume, TaggedProvide};
use zzop_core::{disable_hint, Finding, Severity};

use super::route_near_miss::NearMissTargetRef;
use super::{is_write_method, split_key};

pub fn unconsumed_mutation_endpoint_findings(
    unconsumed_provides: &[TaggedProvide],
    unresolved_consumes: &[TaggedConsume],
    blind_sources: &BTreeSet<String>,
    near_miss_targets: &BTreeMap<(String, String, u32), NearMissTargetRef>,
    trpc_participating_sources: &BTreeSet<String>,
) -> Vec<Finding> {
    let unresolved_http = unresolved_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
        .count();

    // Run-level, not per-provide: "is this run's consume side blind at all" is the question, since a blind
    // source ANYWHERE in the run is a plausible unseen caller of ANY write route regardless of which tree
    // provides it (see this rule's module doc's "Confidence downgrade" section).
    let severity = if blind_sources.is_empty() {
        Severity::Warning
    } else {
        Severity::Info
    };
    let downgrade_note = if blind_sources.is_empty() {
        String::new()
    } else {
        let named: Vec<String> = blind_sources
            .iter()
            .take(3)
            .map(|s| format!("`{s}`"))
            .collect();
        let more = blind_sources.len() - named.len();
        let more_note = if more > 0 {
            format!(", and {more} more")
        } else {
            String::new()
        };
        format!(
            " This run's consume side is partly blind — source(s) {}{more_note} have majority-unresolved \
             `http` consumes (see `cross-layer/unresolved-consume-ratio`) — so severity here is reduced to \
             info: \"unconsumed\" cannot be trusted as a confident zero, and this write endpoint may well be \
             called through one of those unresolved URLs. Confirm before treating it as attack surface.",
            named.join(", ")
        )
    };

    let mut out: Vec<Finding> = unconsumed_provides
        .iter()
        .filter(|p| p.provide.kind == "http" && !zzop_core::is_test_file(&p.provide.file))
        .filter(|p| {
            !(trpc_participating_sources.contains(&p.source)
                && super::is_trpc_mount_route_key(&p.provide.key))
        })
        .filter_map(|p| {
            let (method, _path) = split_key(&p.provide.key)?;
            if !is_write_method(method) {
                return None;
            }
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
                "write endpoint `{key}` (source `{}`) is not called by any source in this analysis. Because it \
                 mutates state, an unconsumed write route is standing attack surface — reachable by anyone \
                 who finds it — not just dead code. That said, this analysis cannot see every caller: a repo \
                 not included in this `analyzeTrees` run, a mobile/native client, a webhook sender, or one of \
                 the {unresolved_http} unresolved dynamic-URL http consume(s) this run could not statically \
                 match to a key (see `crossLayer.unresolvedConsumes`) may still call it. This finding \
                 intentionally co-fires with `cross-layer/unconsumed-endpoint` for the same site — this rule \
                 is the severity-split for write verbs specifically. Confirm with real traffic/access logs \
                 before removing the route, or add authorization/rate-limiting if it must stay reachable.\
                 {near_miss_note}{downgrade_note} {} if provider-only write endpoints (webhook targets, endpoints consumed only outside this \
                 analysis) are expected in your stack.",
                p.source,
                disable_hint("cross-layer/unconsumed-mutation-endpoint")
            );
            let mut data = serde_json::json!({
                "key": key,
                "source": p.source,
                "method": method,
                "symbol": p.provide.symbol,
                "unresolvedHttpConsumeCount": unresolved_http,
            });
            if let Some(t) = near_miss {
                data["nearMissConsumeCount"] = serde_json::json!(t.count);
                data["nearMissConsumeExample"] =
                    serde_json::json!(format!("{}:{}", t.consume_file, t.consume_line));
            }
            Some(Finding {
                rule_id: "cross-layer/unconsumed-mutation-endpoint".to_string(),
                severity,
                file: p.provide.file.clone(),
                line: p.provide.line,
                message,
                data: Some(data),
            })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests;
