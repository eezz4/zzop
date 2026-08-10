//! Tool-output shaping: summary first, capped lists, EXPLICIT truncation disclosure. This is the
//! token-bomb guard for MCP responses, built to never lie by omission: full counts always ride along,
//! every applied cap announces `{shown, totalMatching, hint}` (a silent cap would read as "that's
//! everything"), warnings are never capped (the honest self-report channel outranks brevity), and
//! ordering is deterministic — severity rank descending, original engine order as the tiebreak — so
//! the same analysis produces byte-identical tool output.

/// Default cap for findings lists. Deliberately small: the default answer is a summary an agent can
/// reason over; the `severity`/`rule`/`limit` tool arguments are the drill-down.
const DEFAULT_FINDINGS_LIMIT: usize = 50;
/// Default cap for cross-layer edge lists (edges are small rows; agents usually want them all).
pub(crate) const DEFAULT_EDGES_LIMIT: usize = 200;
/// Default cap for `analyze_repo`'s `degraded` file-path list. `coverage.degraded` already carries the
/// full count as an uncapped scalar, so this list is supplementary detail (which files, not just how
/// many) — a large repo's full degraded-path list must never bypass the same shaping every other list
/// gets (the token-bomb guard this module exists for).
pub const DEFAULT_DEGRADED_LIMIT: usize = 50;
/// Upper bound for a caller-supplied `limit` — keeps a single tool reply bounded no matter what.
const MAX_LIMIT: usize = 1000;

mod bucket_keys;
mod disclosure;
mod filters;
#[cfg(test)]
mod tests;
mod timings;

pub(crate) use bucket_keys::{distinct_bucket_keys, KEY_BUCKETS};
pub(crate) use disclosure::fold as fold_disclosure;
pub(crate) use filters::severity_rank;
pub use filters::FindingFilters;
pub(crate) use timings::shape_rule_timings;
pub use timings::RunKnobs;

/// Shapes a findings array into `{total, bySeverity, byRule, shown, truncated?}`.
/// Counts are ALWAYS over the full set (the summary never shrinks with the filter); `shown` is the
/// filtered, severity-desc-sorted, capped list; `truncated` appears ONLY when `shown` is incomplete.
pub(crate) fn shape_findings(
    findings: &[serde_json::Value],
    filters: &FindingFilters,
) -> serde_json::Value {
    let mut by_severity: std::collections::BTreeMap<String, usize> = Default::default();
    let mut by_rule: std::collections::BTreeMap<String, usize> = Default::default();
    for f in findings {
        let sev = f.get("severity").and_then(|v| v.as_str()).unwrap_or("");
        let rule = f.get("ruleId").and_then(|v| v.as_str()).unwrap_or("");
        *by_severity.entry(sev.to_string()).or_default() += 1;
        *by_rule.entry(rule.to_string()).or_default() += 1;
    }

    let min_rank = filters
        .min_severity
        .as_deref()
        .map(severity_rank)
        .unwrap_or(0);
    let mut matching: Vec<(usize, &serde_json::Value, u8)> = findings
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let sev = f.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            if severity_rank(sev) < min_rank {
                return false;
            }
            match &filters.rule {
                Some(rule) => f.get("ruleId").and_then(|v| v.as_str()) == Some(rule.as_str()),
                None => true,
            }
        })
        .map(|(i, f)| {
            let sev = f.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            (i, f, severity_rank(sev))
        })
        .collect();
    // Severity-desc, then PRODUCTION before TEST-PATH within a tier, original engine order as the
    // stable tiebreak — deterministic. The test-path demotion is the 2026-08-09 U78 ruling: the
    // credential rules deliberately keep scanning test paths (the catalog states that policy — a
    // committed secret is a leak wherever it sits), but a first run's screen should lead with
    // production findings rather than a wall of `password123` fixtures. Shaping-only: nothing is
    // dropped, counts never move, and the demotion announces itself via `testPaths` below. The
    // classifier is the DSL's own shared `test-paths` fragment (via `zzop_facade::test_path_re`) —
    // one owner, so this ordering and the packs' `${test-paths}` exclusions cannot disagree about
    // what a test path is.
    let is_test = |f: &serde_json::Value| {
        f.get("file")
            .and_then(|v| v.as_str())
            .is_some_and(|p| zzop_facade::test_path_re().is_match(p))
    };
    matching.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then(is_test(a.1).cmp(&is_test(b.1)))
            .then(a.0.cmp(&b.0))
    });

    let total_matching = matching.len();
    let limit = filters.limit.unwrap_or(DEFAULT_FINDINGS_LIMIT);
    let shown: Vec<serde_json::Value> = matching
        .iter()
        .take(limit)
        .map(|(_, f, _)| (*f).clone())
        .collect();

    let mut out = serde_json::json!({
        "total": findings.len(),
        "bySeverity": by_severity,
        "byRule": by_rule,
        "shown": shown,
    });
    // Additive-only, like `truncated`: present exactly when it has something to say. Counted over the
    // FULL set (the same contract as `bySeverity`/`byRule`), not the filtered one, so the number a
    // reader quotes does not shrink with their filter.
    let test_path_count = findings.iter().filter(|f| is_test(f)).count();
    if test_path_count > 0 {
        out["testPaths"] = serde_json::json!({
            "count": test_path_count,
            "meaning": "findings whose file is a test path (the DSL's shared test-paths pattern). \
                        They are still real findings — a committed credential is a leak wherever it \
                        sits, and rules that scan test paths say so in the catalog — but within each \
                        severity tier they sort after production findings, so a first screen leads \
                        with production code. Nothing is dropped; every count includes them.",
        });
    }
    if total_matching > limit {
        // The one surface where all three tool arguments really do move the cap — `shape_list`'s
        // callers must NOT reuse this hint (see `shape_list`).
        out["truncated"] = truncation(
            limit,
            total_matching,
            "narrow by severity or rule, or raise the limit",
        );
    }
    // Zero-match rule-filter disclosure: `shown: []` from a real rule with zero findings this run is
    // indistinguishable from `shown: []` from a TYPO'd/nonexistent rule id — both look identical on the
    // wire. When a `rule` filter is present and matched nothing, cross-check it against `byRule` (built
    // from the FULL, unfiltered set above): a rule id absent from `byRule` never fired at all this run,
    // so the filter is almost certainly wrong rather than merely quiet. Deterministic and additive-only
    // (never fires when the filter matched >=1 finding, never touches `shown`/`truncated`).
    if let Some(rule) = &filters.rule {
        if total_matching == 0 && !by_rule.contains_key(rule.as_str()) {
            out["note"] = serde_json::Value::String(format!(
                // Names the DOCUMENT, not a route to it. This sentence used to end "read it via the
                // zzop://contract/rule-catalog resource or `zzop contract rule-catalog`" — one route
                // per host, which is better than naming only one but still worse than naming neither:
                // every host serves `rule-catalog` by that name, so the name IS the answer on both,
                // and a route list has to grow every time a surface does. Same call the starter config
                // took when the host-vocabulary contract caught two lines in it. This message was
                // invisible to that contract until 2026-07-28 — the scan truncated the file at a
                // `#[cfg(test)] mod` DECLARATION, hiding 91% of it.
                "rule filter '{rule}' matched no findings and is not among this run's fired rule ids — \
                 check the id (byRule lists what fired; the `rule-catalog` contract document lists all \
                 of them, and every zzop surface serves it under that name)"
            ));
        }
    }
    out
}

/// Shapes a plain list (edges, ...) into `(shown, truncated?)` with the same disclosure contract.
///
/// The caller passes the `hint`, and it must name a remedy that ACTUALLY works on this list. The
/// shared `severity`/`rule`/`limit` tool arguments reach `shape_findings` and nothing else — every
/// `shape_list` cap is a fixed constant no tool argument can move — so telling a caller here to "raise
/// limit" is advice that silently does nothing. A disclosure whose remedy is inert is worse than a
/// bare count: it reads as actionable and burns a round-trip proving otherwise (the same class as a
/// suppression marker documented at a line the scanner does not read).
pub(crate) fn shape_list(
    items: &[serde_json::Value],
    limit: usize,
    hint: &str,
) -> (Vec<serde_json::Value>, Option<serde_json::Value>) {
    let shown: Vec<serde_json::Value> = items.iter().take(limit).cloned().collect();
    let truncated = (items.len() > limit).then(|| truncation(limit, items.len(), hint));
    (shown, truncated)
}

fn truncation(shown: usize, total_matching: usize, hint: &str) -> serde_json::Value {
    serde_json::json!({
        "shown": shown,
        "totalMatching": total_matching,
        "hint": hint,
    })
}
