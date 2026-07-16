//! `cross-layer/prefix-drift` (info) — a pure aggregation over `route_near_miss`'s `prefix_records`. Real
//! dogfood (fe-axios x be-nest) produces 19 independent `cross-layer/route-near-miss` findings that all
//! share ONE root cause: the backend serves under a `/api` global prefix the frontend calls omit. Reporting
//! 19 near-identical findings is tone noise, not signal — this rule groups prefix near-misses by
//! `(consume_source, provide_source, prefix, consume_missing_prefix)` and, for any group of
//! `MIN_PREFIX_DRIFT_GROUP` or more, emits ONE `cross-layer/prefix-drift` finding that names the single
//! likely base-path cause instead of N independent route drifts.
//!
//! Info severity: a shared prefix can be a legitimate gateway/proxy rewrite, or a baseURL that already
//! includes the prefix at the transport layer — this is a "verify once" signal, not confirmed drift. That
//! keeps it FP-safe and out of any `failOn` CI gate, same as the sibling near-miss rules it aggregates.
//!
//! The orchestrator (the engine call site, not this module) suppresses the subsumed per-route
//! `cross-layer/route-near-miss` findings for consumes folded into a fired aggregate here, using
//! [`retain_non_subsumed`]. No information is lost in that suppression: this finding enumerates every
//! collapsed route in its `data.routes` and message body (`output-philosophy.md` §0/§1 — no silent
//! suppression, only replacement of N findings with one that discloses all N).

use std::collections::BTreeMap;

use zzop_core::{disable_hint, Finding, Severity};

use super::route_near_miss::PrefixNearMissRecord;

/// Minimum prefix near-misses sharing one (consume-source, provide-source, prefix, direction) before the
/// aggregate fires. 2 can be coincidence; 3+ is a base-path/gateway/baseURL pattern. Policy value — pinned
/// by `fires_at_threshold_not_below`. (rule-quality.md §6 inventory.)
pub const MIN_PREFIX_DRIFT_GROUP: usize = 3;

/// `prefix_drift_findings`'s return: the aggregate findings, plus the `subsumed` set the orchestrator uses
/// to drop the per-route `route-near-miss` findings that fired aggregates now replace.
pub struct PrefixDriftOutput {
    pub findings: Vec<Finding>,
    /// (consume_source, consume_file, consume_line, consume_key) of every consume folded into a fired
    /// aggregate — the orchestrator drops the matching per-route `route-near-miss` findings. The
    /// `consume_key` is part of the key on purpose: a single physical source line can host two distinct
    /// `http` consumes (`Promise.all([api.get('/articles'), api.get('/Articles')])`), one folded here and
    /// one an independent (e.g. `case`-dimension) near-miss — keying on `(source, file, line)` alone would
    /// drop the second too, a silent suppression the aggregate never discloses (`output-philosophy.md`
    /// §0/§1). The exact consume key pins the match to the folded consume only.
    pub subsumed: std::collections::BTreeSet<(String, String, u32, String)>,
}

/// Groups `records` by `(consume_source, provide_source, prefix, consume_missing_prefix)` and emits one
/// `cross-layer/prefix-drift` finding per group that reaches `MIN_PREFIX_DRIFT_GROUP`. Pure aggregation —
/// never re-runs the route-near-miss match itself (see module doc; `records` is the single source of
/// truth, produced by `route_near_miss_results`).
pub fn prefix_drift_findings(records: &[PrefixNearMissRecord]) -> PrefixDriftOutput {
    let mut groups: BTreeMap<(String, String, String, bool), Vec<&PrefixNearMissRecord>> =
        BTreeMap::new();
    for r in records {
        groups
            .entry((
                r.consume_source.clone(),
                r.provide_source.clone(),
                r.prefix.clone(),
                r.consume_missing_prefix,
            ))
            .or_default()
            .push(r);
    }

    let mut findings = Vec::new();
    let mut subsumed: std::collections::BTreeSet<(String, String, u32, String)> =
        std::collections::BTreeSet::new();

    for ((consume_source, provide_source, prefix, consume_missing_prefix), mut group) in groups {
        if group.len() < MIN_PREFIX_DRIFT_GROUP {
            continue;
        }
        group.sort_by(|a, b| {
            a.consume_file
                .cmp(&b.consume_file)
                .then(a.consume_line.cmp(&b.consume_line))
        });
        let anchor = group[0];

        let mut routes: Vec<&str> = group.iter().map(|r| r.consume_key.as_str()).collect();
        routes.sort_unstable();
        routes.dedup();

        for r in &group {
            subsumed.insert((
                r.consume_source.clone(),
                r.consume_file.clone(),
                r.consume_line,
                r.consume_key.clone(),
            ));
        }

        let n = group.len();
        let (article, verb) = if consume_missing_prefix {
            ("a missing", "add")
        } else {
            ("an extra", "remove")
        };
        let provide_example_key = anchor.provide_key.clone();

        let message = format!(
            "{n} consumes from `{consume_source}` have no exact provider, but each matches a \
             `{provide_source}` route once you account for {article} path prefix (`{prefix}`) — one likely \
             base-path mismatch (a global route prefix like NestJS `setGlobalPrefix`, a gateway/proxy \
             rewrite, or an axios/fetch baseURL that includes `{prefix}`), not {n} independent route drifts. \
             Align the base path once ({verb} `{prefix}` on one side). This replaces the per-route \
             `cross-layer/route-near-miss` findings for these calls; affected routes: {}. Verify manually — \
             a helper/wrapper can make the runtime request differ from the call site. {}",
            routes.join(", "),
            disable_hint("cross-layer/prefix-drift"),
        );

        findings.push(Finding {
            rule_id: "cross-layer/prefix-drift".to_string(),
            severity: Severity::Info,
            file: anchor.consume_file.clone(),
            line: anchor.consume_line,
            message,
            data: Some(serde_json::json!({
                "consumeSource": consume_source,
                "provideSource": provide_source,
                "prefix": prefix,
                "consumeMissingPrefix": consume_missing_prefix,
                "routeCount": n,
                "routes": routes,
                "provideExampleKey": provide_example_key,
            })),
        });
    }

    findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    PrefixDriftOutput { findings, subsumed }
}

/// Drops the per-route `cross-layer/route-near-miss` findings whose consume was folded into a fired
/// prefix-drift aggregate. Matches on the finding's `data.consumeSource` + `data.consumeKey` (route_near_miss's
/// own data keys) plus its `file`/`line` anchor — the exact `(source, file, line, key)` the aggregate
/// recorded, so a second, independent near-miss sharing the same source line but a different consume key is
/// NOT dropped (see `PrefixDriftOutput::subsumed`). Findings missing either data key are kept (defensive).
pub fn retain_non_subsumed(
    findings: Vec<Finding>,
    subsumed: &std::collections::BTreeSet<(String, String, u32, String)>,
) -> Vec<Finding> {
    findings
        .into_iter()
        .filter(|f| {
            let data = f.data.as_ref();
            let consume_source = data
                .and_then(|d| d.get("consumeSource"))
                .and_then(|v| v.as_str());
            let consume_key = data
                .and_then(|d| d.get("consumeKey"))
                .and_then(|v| v.as_str());
            let (Some(consume_source), Some(consume_key)) = (consume_source, consume_key) else {
                return true;
            };
            !subsumed.contains(&(
                consume_source.to_string(),
                f.file.clone(),
                f.line,
                consume_key.to_string(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests;
