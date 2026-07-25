//! `cross-layer/unconsumed-mutation-endpoint` (warning, downgraded to info when the run has a blind consume
//! side) — one finding per unconsumed write-verb HTTP provide (`is_write_method`: POST/PUT/PATCH/DELETE): an
//! endpoint that MUTATES state and that no source in this analysis calls. An unconsumed write endpoint is
//! standing attack surface — reachable by anyone who finds it — not merely dead code, hence a warning here
//! versus the plain info of `cross-layer/unconsumed-endpoint`.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — not deployed surface.
//!
//! ## This rule REPLACES `unconsumed-endpoint` on the routes it reports
//! A write route is an endpoint, so this rule is a strict specialization of `cross-layer/unconsumed-endpoint`
//! — and the two used to report the identical `file:line` twice (dogfood measured `POST /api/ledger/{}/verify`
//! billed as two problems). The general rule now stays silent on exactly the ROUTES this rule REPORTED, read
//! off this rule's own output via [`reported_provide_sites`] at the `zzop_engine::cross_layer_findings` call
//! site — not from a second copy of the write-verb predicate, which would drift apart from
//! [`is_mutation_endpoint_site`] the moment either rule's exclusions changed. Keying on real output also makes
//! the handoff gate-aware for free: when THIS rule is disabled it produces no sites, nothing is suppressed,
//! and the general rule reports write routes itself — disabling one rule must never punch a silent hole in a
//! rule the user did not disable. The handoff carries the interface key, not just the `file:line` anchor,
//! because a verb-agnostic registration puts several routes on one line — see [`reported_provide_sites`].
//!
//! ## Confidence downgrade when the run is blind
//! A zero ("unconsumed") is only a confident zero when the consume key space was actually resolved
//! (`output-philosophy.md` §1). When `blind_sources` (`super::majority_unresolved_http_sources`, the same
//! predicate `unresolved_consume_ratio` uses to self-report) is non-empty for this run, "unconsumed" cannot
//! be trusted as a confident zero — this rule de-escalates to `Severity::Info` and names the blind source(s)
//! in the message instead of silently keeping Warning. This is a de-escalation to match confidence,
//! NOT suppression — the finding still fires either way (`output-philosophy.md` §0: total by default).
//! The note is placed immediately after the endpoint identification, ahead of the risk framing, rather than
//! trailing a long paragraph: a field reviewer once read the Warning/Info difference between two run modes as
//! a bug because the sentence explaining it sat at the end of the message. Placement only — the downgrade
//! itself is deliberate and must stay.
//!
//! ## The NO-downgrade branch speaks too (severity-gate framing, `output-philosophy.md` §0)
//! The zero-blind-sources branch used to emit an EMPTY note, so warning severity had to be read as "zzop
//! checked and the caller set was complete" — the narrow-detector-implies-completeness inference the
//! downgrade exists to avoid, reappearing one layer up as silence. `blind_sources` is a single narrow
//! predicate (a source whose `http` consumes are MAJORITY-unresolved); it cannot see a source with a
//! minority of unresolved consumes, a caller in a call shape or language this extraction does not model, or
//! a caller outside the run. So the empty branch now states what warning severity is conditioned on:
//! blindness was not WITNESSED, which is not a completeness proof. Framing only — the doctrine
//! (confidence = witnessed coverage, no dynamic census) and the gate itself are unchanged, and the finding
//! count and severity are byte-identical to before.
//!
//! ## Near-miss cross-reference
//! Same annotation as the sibling `unconsumed_endpoint`: when a write provide here is ALSO the chosen
//! near-miss target of an unmatched `cross-layer/route-near-miss` consume (`near_miss_targets`, sourced from
//! `route_near_miss::route_near_miss_results`), the message gains a cross-reference note pointing at that
//! finding — see `unconsumed_endpoint`'s module doc for the dogfood motivation.
//!
//! ## tRPC mount-route suppression
//! Same exclusion as the sibling `unconsumed_endpoint` (see its module doc): an EXPLICIT-verb tRPC mount
//! provide [`super::is_trpc_mount_route_key`] identifies (e.g. an app-router `route.ts` `export const POST`)
//! is excluded here when ITS OWN source tree is in `trpc_participating_sources` — it would otherwise fire
//! this write-verb rule for a tone-noise transport site. A serve-all `pages/api` mount is instead a
//! verb-unknown `UNKNOWN_VERB` sentinel (1b) that carries no write method and is partitioned out of the
//! provide universe upstream, so it never reaches this rule. Per-tree, not run-global: see
//! `unconsumed_endpoint`'s module doc for why a run-global edge count would misattribute suppression.
//!
//! Note that the externally-fetched-path veto `unconsumed_endpoint` applies (`/`, health probes,
//! `robots.txt`, feeds, …) deliberately does NOT apply here: every requester that justifies that veto — an
//! uptime monitor, a browser, a crawler, a feed reader — issues a READ. A write route sitting at such a path
//! has no by-definition external caller, so silence there would be unearned.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::{TaggedConsume, TaggedProvide};
use zzop_core::{disable_hint, Finding, Severity};

use super::route_near_miss::NearMissTargetRef;
use super::{is_write_method, split_key};

/// This rule's id, shared by the producer below and by [`reported_provide_sites`] so the reader can never
/// select the wrong findings out of a mixed slice.
const RULE_ID: &str = "cross-layer/unconsumed-mutation-endpoint";

/// The whole site set this rule reports, exclusions included, in one place — so an exclusion is added once
/// rather than to a filter chain that a sibling has to mirror.
fn is_mutation_endpoint_site(
    p: &TaggedProvide,
    trpc_participating_sources: &BTreeSet<String>,
) -> bool {
    p.provide.kind == "http"
        && !zzop_core::is_test_file(&p.provide.file)
        && !(trpc_participating_sources.contains(&p.source)
            && super::is_trpc_mount_route_key(&p.provide.key))
        && split_key(&p.provide.key).is_some_and(|(method, _)| is_write_method(method))
}

/// The provides this rule ACTUALLY reported, as `(source, file, line, key)`. Read back off the findings
/// (their anchor plus their own `data.source`/`data.key`), never recomputed from the provide list, which is
/// what makes `unconsumed_endpoint`'s suppression track this rule's real output instead of a predicate copy
/// — including "reported nothing because I am disabled", where the caller simply never builds this set.
/// Lives next to the producer so the `data` keys it reads cannot drift from the ones written below.
///
/// The interface KEY is part of the tuple, not just the `(source, file, line)` anchor, because one anchor is
/// routinely several routes: a verb-agnostic registration expands to one provide PER method at a single line
/// (gin `router.Any`, axum `any(...)`, the TS pathname-dispatch adapter emitting every scanned verb at the
/// path test's line). Keyed on the anchor alone, reporting `POST /webhook` would also silence the co-located
/// `GET /webhook` — in NEITHER rule, the silent hole this handoff exists to avoid. Suppression must identify
/// the ROUTE it replaces, never merely the source line it sits on.
pub fn reported_provide_sites(findings: &[Finding]) -> BTreeSet<(String, String, u32, String)> {
    findings
        .iter()
        .filter(|f| f.rule_id == RULE_ID)
        .filter_map(|f| {
            let data = f.data.as_ref()?;
            let source = data.get("source")?.as_str()?;
            let key = data.get("key")?.as_str()?;
            Some((source.to_string(), f.file.clone(), f.line, key.to_string()))
        })
        .collect()
}

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
    // Both branches speak. The empty branch used to be silent, which left warning severity reading as a
    // proof of completeness ("no blindness detected => the caller set was resolved") — the exact
    // class-extrapolation the non-empty branch exists to avoid. The check that did not fire is narrow, so
    // its silence is named rather than inferred (`output-philosophy.md` §0).
    let confidence_note = if blind_sources.is_empty() {
        " Severity is warning because no source in this run tripped the consume-side blindness check — \
         that check only asks whether a source's `http` consumes are majority-unresolved, so its not \
         firing means no blindness was WITNESSED, not that the caller set was proven complete."
            .to_string()
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
        .filter(|p| is_mutation_endpoint_site(p, trpc_participating_sources))
        .filter_map(|p| {
            let (method, _path) = split_key(&p.provide.key)?;
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
                "write endpoint `{key}` (source `{}`) is not called by any source in this analysis.\
                 {confidence_note} Because it mutates state, an unconsumed write route is standing attack \
                 surface — reachable by anyone who finds it — not just dead code. That said, this analysis \
                 cannot see every caller: a repo not included in this `analyzeTrees` run, a mobile/native \
                 client, a webhook sender, or one of the {unresolved_http} unresolved dynamic-URL http \
                 consume(s) this run could not statically match to a key (see \
                 `crossLayer.unresolvedConsumes`) may still call it. `cross-layer/unconsumed-endpoint` stays \
                 silent on this route by design — this rule is its write-verb specialization and reports \
                 the write subset in its place, so one route never yields two findings. Other routes \
                 registered on the same line are unaffected. Confirm with real \
                 traffic/access logs before removing the route, or add authorization/rate-limiting if it must \
                 stay reachable.{near_miss_note} {} if provider-only write endpoints (webhook targets, \
                 endpoints consumed only outside this analysis) are expected in your stack.",
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
                rule_id: RULE_ID.to_string(),
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
