//! `cross-layer/route-near-miss` (info) — an unprovided http consume whose key differs from a same-method
//! provide by EXACTLY ONE structural dimension: `case` (a path segment differs only in letter casing) or
//! `prefix` (the shorter path is an exact suffix of the longer, with a 1-2 all-literal leading prefix
//! added/removed — a base-path like `/api` or `/api/v1`, the classic `setGlobalPrefix` drift). Deliberately
//! disjoint from `path_near_miss`: that rule owns the SAME segment count, every-segment-equal-or-`{}` case
//! (pure parameter generalization) — this rule never fires on a pair `path_near_miss` would already explain,
//! and neither dimension here can produce a `path_near_miss`-shaped pair (case requires an exact-case
//! mismatch on a same-length pair, which `path_near_miss`'s equal-or-`{}` test already rejects; prefix
//! requires a segment-count difference, which `path_near_miss` requires to be absent).
//!
//! Info severity (same as the sibling `path_near_miss`): these are honest "verify manually" near-misses, not
//! confirmed drift — a same-method one-dimension-apart provide is strong evidence but a wrapper can make the
//! runtime request differ from the call site, and unrelated routes can be one dimension apart by coincidence.
//! Info keeps the rule FP-safe while still surfacing the near-miss in the cross-repo report.
//!
//! An earlier draft carried a third `arity` dimension (literal segments equal, `{}` parameter count differs).
//! It was dropped: literal-segments-equal-plus-differing-`{}`-count is dominated by legitimate REST
//! collection/item and nested-resource shapes (`GET /users` vs `GET /users/{}`, `GET /{orgId}` vs
//! `GET /{orgId}/{repoId}`) — those are distinct endpoints, not near-misses — so arity was mostly-FP even at
//! Info.

use std::collections::BTreeMap;

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::{is_all_slot_path, path_segments, split_key, HttpProvideSite};

mod dimensions;
#[cfg(test)]
mod tests;

use dimensions::{
    case_dimension_match, is_path_near_miss_pair, prefix_dimension_match, provide_path_segs,
    Dimension,
};

/// The cross-reference data `unconsumed_endpoint`/`unconsumed_mutation_endpoint` attach to a provide that is
/// ALSO the chosen near-miss target of one or more unprovided consumes — see `RouteNearMissOutput`.
/// `consume_file`/`consume_line` anchor the FIRST such consume in input order (stable regardless of how many
/// consumes name the same provide); `count` is the total.
#[derive(Debug, Clone)]
pub struct NearMissTargetRef {
    pub consume_file: String,
    pub consume_line: u32,
    pub count: u32,
}

/// One `prefix`-dimension route-near-miss, exposed as typed data so `prefix_drift` can aggregate these
/// without re-running the match (single source of truth — see `RouteNearMissOutput::prefix_records`).
#[derive(Debug, Clone)]
pub struct PrefixNearMissRecord {
    pub consume_source: String,
    pub consume_key: String, // e.g. "GET /articles"
    pub consume_file: String,
    pub consume_line: u32,
    pub provide_source: String,
    pub provide_key: String, // e.g. "GET /api/articles"
    pub prefix: String,      // e.g. "/api"
    /// true = the consume is MISSING the prefix the provide has (consume shorter); false = the consume
    /// carries an EXTRA prefix the provide lacks.
    pub consume_missing_prefix: bool,
}

/// `route_near_miss_results`'s return: the findings (byte-identical to `route_near_miss_findings`'s output)
/// plus the provide-site -> near-miss cross-reference `targets` map that lets the sibling unconsumed-* rules
/// annotate a dead-looking provide that is actually a live near-miss target. `targets` is keyed on the
/// CHOSEN `first` candidate of each finding only — the `extra` (`otherNearMissCount`) candidates near it are
/// NOT keys, since only `first` is what the finding's message actually names as "the" near-miss provide.
/// `BTreeMap` keyed by `(source, file, line)` for deterministic iteration order.
pub struct RouteNearMissOutput {
    pub findings: Vec<Finding>,
    pub targets: BTreeMap<(String, String, u32), NearMissTargetRef>,
    /// Every `prefix`-dimension near-miss this run found, typed (not re-derived from `findings`'s `data`
    /// JSON) so `prefix_drift` can aggregate them directly — see `PrefixNearMissRecord`.
    pub prefix_records: Vec<PrefixNearMissRecord>,
}

/// Does the actual candidate-selection work for `cross-layer/route-near-miss`, single-sourced so the
/// findings and the near-miss cross-reference map (`targets`) can never drift apart — see
/// `RouteNearMissOutput`.
pub fn route_near_miss_results(
    unprovided_consumes: &[TaggedConsume],
    all_provides: &[HttpProvideSite],
) -> RouteNearMissOutput {
    let mut out = Vec::new();
    let mut targets: BTreeMap<(String, String, u32), NearMissTargetRef> = BTreeMap::new();
    let mut prefix_records: Vec<PrefixNearMissRecord> = Vec::new();
    for c in unprovided_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
    {
        let Some(key) = c.consume.key.as_deref() else {
            continue;
        };
        let Some((method, path)) = split_key(key) else {
            continue;
        };
        let consume_segs = path_segments(path);
        if is_all_slot_path(&consume_segs) {
            // Minimum-information gate: an all-`{}` consume (typically a head-drop artifact) carries
            // zero literal evidence, so it can vacuously satisfy the prefix dimension against any
            // same-shaped provide — see `is_all_slot_path`'s doc.
            continue;
        }

        // Priority order: case > prefix. As soon as one dimension has at least one candidate, stop — the
        // lower-priority dimension never gets consulted for this consume.
        let mut chosen: Option<(Dimension, Vec<&HttpProvideSite>)> = None;
        for dimension in [Dimension::Case, Dimension::Prefix] {
            let mut candidates: Vec<&HttpProvideSite> = Vec::new();
            for p in all_provides {
                let Some((pmethod, provide_segs)) = provide_path_segs(p) else {
                    continue;
                };
                if pmethod != method {
                    continue;
                }
                if is_path_near_miss_pair(&consume_segs, &provide_segs) {
                    continue;
                }
                let matched = match dimension {
                    Dimension::Case => case_dimension_match(&consume_segs, &provide_segs),
                    Dimension::Prefix => {
                        prefix_dimension_match(&consume_segs, &provide_segs).is_some()
                    }
                };
                if matched {
                    candidates.push(p);
                }
            }
            if !candidates.is_empty() {
                chosen = Some((dimension, candidates));
                break;
            }
        }

        let Some((dimension, mut candidates)) = chosen else {
            continue;
        };
        candidates.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });
        let first = candidates[0];
        let extra = candidates.len() - 1;

        // Record the cross-reference: the FIRST consume (input order) to choose this provide as its
        // near-miss target anchors `consume_file`/`consume_line`; every later consume choosing the same
        // provide only bumps `count`.
        targets
            .entry((first.source.clone(), first.file.clone(), first.line))
            .and_modify(|t| t.count += 1)
            .or_insert(NearMissTargetRef {
                consume_file: c.consume.file.clone(),
                consume_line: c.consume.line,
                count: 1,
            });
        let extra_note = if extra > 0 {
            format!(" (and {extra} other near-miss route(s))")
        } else {
            String::new()
        };

        // Recompute the dimension-specific detail from `first` itself (not from whatever candidate was
        // encountered first during the scan above) — with multiple candidates on the same dimension, they
        // can carry different prefixes, so the message must describe the ACTUAL chosen provide.
        let dimension_detail = match dimension {
            Dimension::Case => "differs only by path segment letter casing — the segments match \
                 case-insensitively but not case-sensitively"
                .to_string(),
            Dimension::Prefix => {
                let first_provide_segs =
                    provide_path_segs(first).map(|(_, s)| s).unwrap_or_default();
                let prefix_segments = prefix_dimension_match(&consume_segs, &first_provide_segs)
                    .expect("first was selected as a prefix-dimension candidate");
                let prefix_str = format!("/{}", prefix_segments.join("/"));
                let consume_missing_prefix = consume_segs.len() < first_provide_segs.len();
                prefix_records.push(PrefixNearMissRecord {
                    consume_source: c.source.clone(),
                    consume_key: format!("{method} {path}"),
                    consume_file: c.consume.file.clone(),
                    consume_line: c.consume.line,
                    provide_source: first.source.clone(),
                    provide_key: first.key.clone(),
                    prefix: prefix_str.clone(),
                    consume_missing_prefix,
                });
                if consume_missing_prefix {
                    format!(
                        "differs only by a missing path prefix (`{prefix_str}`) — the consume is missing a \
                         leading segment the provide has"
                    )
                } else {
                    format!(
                        "differs only by an extra path prefix (`{prefix_str}`) — the consume carries a \
                         leading segment the provide does not have"
                    )
                }
            }
        };

        let message = format!(
            "consume `{method} {path}` (source `{}`) has no exact provider, but `{}` provides `{}` at \
             {}:{}{extra_note} — {dimension_detail}. This could be genuine route drift (align the call path \
             with the served route, or vice versa), or two unrelated routes that happen to be one dimension \
             apart — verify manually before treating this as drift. A prefix difference in particular can be \
             deployment topology the source does not carry — a gateway/ingress mount prefix or a config-file \
             path rewrite zzop does not read; inject it via `mounts`/`mountedAt`/`hosts` if so. The \
             consume-side method and path reflect \
             what static extraction read at the call site; a helper/wrapper around the call can make them \
             differ from the runtime request. {} if one-dimension-apart-but-unrelated routes are common in \
             your stack.",
            c.source, first.source, first.key, first.file, first.line,
            disable_hint("cross-layer/route-near-miss"),
        );

        out.push(Finding {
            rule_id: "cross-layer/route-near-miss".to_string(),
            severity: Severity::Info,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            data: Some(serde_json::json!({
                "consumeKey": key,
                "consumeSource": c.source,
                "dimension": dimension.as_str(),
                "nearMissProvide": {"source": first.source, "file": first.file, "line": first.line, "key": first.key},
                "otherNearMissCount": extra,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    RouteNearMissOutput {
        findings: out,
        targets,
        prefix_records,
    }
}

/// Thin wrapper over `route_near_miss_results` for callers that only need the findings (existing unit tests,
/// and any caller not threading the near-miss cross-reference into the unconsumed-* rules).
pub fn route_near_miss_findings(
    unprovided_consumes: &[TaggedConsume],
    all_provides: &[HttpProvideSite],
) -> Vec<Finding> {
    route_near_miss_results(unprovided_consumes, all_provides).findings
}
