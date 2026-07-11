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
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn record(
        consume_source: &str,
        consume_key: &str,
        consume_file: &str,
        consume_line: u32,
        provide_source: &str,
        provide_key: &str,
        prefix: &str,
        consume_missing_prefix: bool,
    ) -> PrefixNearMissRecord {
        PrefixNearMissRecord {
            consume_source: consume_source.to_string(),
            consume_key: consume_key.to_string(),
            consume_file: consume_file.to_string(),
            consume_line,
            provide_source: provide_source.to_string(),
            provide_key: provide_key.to_string(),
            prefix: prefix.to_string(),
            consume_missing_prefix,
        }
    }

    #[test]
    fn fires_at_threshold_not_below() {
        let records = vec![
            record(
                "fe",
                "GET /articles",
                "Api.tsx",
                10,
                "be",
                "GET /api/articles",
                "/api",
                true,
            ),
            record(
                "fe",
                "GET /comments",
                "Api.tsx",
                20,
                "be",
                "GET /api/comments",
                "/api",
                true,
            ),
            record(
                "fe",
                "GET /users",
                "Api.tsx",
                30,
                "be",
                "GET /api/users",
                "/api",
                true,
            ),
        ];
        let out = prefix_drift_findings(&records);
        assert_eq!(out.findings.len(), 1);
        let f = &out.findings[0];
        assert_eq!(f.rule_id, "cross-layer/prefix-drift");
        assert_eq!(f.severity, Severity::Info);
        assert_eq!(f.data.as_ref().unwrap()["routeCount"], 3);
        assert!(f.message.contains("/api"));
        assert!(f.message.contains("missing"));
        assert_eq!(out.subsumed.len(), 3);

        // Below threshold (only 2 records): must not fire.
        let below = prefix_drift_findings(&records[..2]);
        assert!(below.findings.is_empty());
        assert!(below.subsumed.is_empty());
    }

    #[test]
    fn groups_are_separated_by_provide_source_prefix_and_direction() {
        // Same consume_source/provide_source, but different prefixes ("/api" vs "/v1") and one differing
        // direction — none of these subgroups reach 3, so nothing fires even though the total is 3+ records.
        let records = vec![
            record(
                "fe",
                "GET /articles",
                "Api.tsx",
                10,
                "be",
                "GET /api/articles",
                "/api",
                true,
            ),
            record(
                "fe",
                "GET /comments",
                "Api.tsx",
                20,
                "be",
                "GET /v1/comments",
                "/v1",
                true,
            ),
            record(
                "fe",
                "GET /api/users",
                "Api.tsx",
                30,
                "be",
                "GET /users",
                "/api",
                false,
            ),
        ];
        let out = prefix_drift_findings(&records);
        assert!(out.findings.is_empty());
        assert!(out.subsumed.is_empty());

        // Bump each subgroup to 3 and confirm each fires independently.
        let mut bigger = records.clone();
        bigger.push(record(
            "fe",
            "GET /widgets",
            "Api.tsx",
            40,
            "be",
            "GET /api/widgets",
            "/api",
            true,
        ));
        bigger.push(record(
            "fe",
            "GET /gadgets",
            "Api.tsx",
            50,
            "be",
            "GET /api/gadgets",
            "/api",
            true,
        ));
        let out2 = prefix_drift_findings(&bigger);
        assert_eq!(out2.findings.len(), 1);
        assert_eq!(out2.findings[0].data.as_ref().unwrap()["prefix"], "/api");
        assert_eq!(
            out2.findings[0].data.as_ref().unwrap()["consumeMissingPrefix"],
            true
        );
    }

    #[test]
    fn extra_prefix_direction_wording() {
        let records = vec![
            record(
                "fe",
                "GET /api/articles",
                "Api.tsx",
                10,
                "be",
                "GET /articles",
                "/api",
                false,
            ),
            record(
                "fe",
                "GET /api/comments",
                "Api.tsx",
                20,
                "be",
                "GET /comments",
                "/api",
                false,
            ),
            record(
                "fe",
                "GET /api/users",
                "Api.tsx",
                30,
                "be",
                "GET /users",
                "/api",
                false,
            ),
        ];
        let out = prefix_drift_findings(&records);
        assert_eq!(out.findings.len(), 1);
        assert!(out.findings[0].message.contains("extra"));
        assert!(out.findings[0].message.contains("remove"));
    }

    fn near_miss_finding(file: &str, line: u32, source: &str, key: &str, msg: &str) -> Finding {
        Finding {
            rule_id: "cross-layer/route-near-miss".to_string(),
            severity: Severity::Info,
            file: file.to_string(),
            line,
            message: msg.to_string(),
            data: Some(serde_json::json!({"consumeSource": source, "consumeKey": key})),
        }
    }

    #[test]
    fn retain_non_subsumed_drops_only_subsumed() {
        let subsumed_finding = near_miss_finding("Api.tsx", 10, "fe", "GET /articles", "subsumed");
        let kept_finding = near_miss_finding("Api.tsx", 999, "fe", "GET /comments", "kept");
        let no_data_finding = Finding {
            rule_id: "cross-layer/route-near-miss".to_string(),
            severity: Severity::Info,
            file: "Api.tsx".to_string(),
            line: 10,
            message: "no data keys".to_string(),
            data: None,
        };

        let mut subsumed = std::collections::BTreeSet::new();
        subsumed.insert((
            "fe".to_string(),
            "Api.tsx".to_string(),
            10,
            "GET /articles".to_string(),
        ));

        let out = retain_non_subsumed(
            vec![
                subsumed_finding.clone(),
                kept_finding.clone(),
                no_data_finding.clone(),
            ],
            &subsumed,
        );
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|f| f.message == "kept"));
        assert!(out.iter().any(|f| f.message == "no data keys"));
        assert!(!out.iter().any(|f| f.message == "subsumed"));
    }

    #[test]
    fn retain_non_subsumed_keeps_a_second_consume_on_the_same_line() {
        // Two distinct consumes on the SAME source+file+line (e.g. `Promise.all([get('/articles'),
        // get('/Articles')])`): one folded into the aggregate (prefix), one an independent case near-miss.
        // Keying on (source, file, line) alone would drop BOTH — the key MUST include consumeKey so only the
        // folded consume is suppressed and the other survives (output-philosophy §0/§1 — no silent drop).
        let folded = near_miss_finding("Api.tsx", 10, "fe", "GET /articles", "folded-prefix");
        let independent =
            near_miss_finding("Api.tsx", 10, "fe", "GET /Articles", "independent-case");

        let mut subsumed = std::collections::BTreeSet::new();
        subsumed.insert((
            "fe".to_string(),
            "Api.tsx".to_string(),
            10,
            "GET /articles".to_string(),
        ));

        let out = retain_non_subsumed(vec![folded, independent], &subsumed);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "independent-case");
    }

    #[test]
    fn deterministic_output() {
        let records = vec![
            record(
                "fe",
                "GET /articles",
                "Api.tsx",
                10,
                "be",
                "GET /api/articles",
                "/api",
                true,
            ),
            record(
                "fe",
                "GET /comments",
                "Api.tsx",
                20,
                "be",
                "GET /api/comments",
                "/api",
                true,
            ),
            record(
                "fe",
                "GET /users",
                "Api.tsx",
                30,
                "be",
                "GET /api/users",
                "/api",
                true,
            ),
        ];
        let out1 = prefix_drift_findings(&records);
        let out2 = prefix_drift_findings(&records);
        assert_eq!(
            out1.findings.iter().map(|f| &f.message).collect::<Vec<_>>(),
            out2.findings.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }
}
