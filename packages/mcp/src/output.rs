//! Tool-output shaping: summary first, capped lists, EXPLICIT truncation disclosure. This is the
//! token-bomb guard for MCP responses, built to never lie by omission: full counts always ride along,
//! every applied cap announces `{shown, totalMatching, hint}` (a silent cap would read as "that's
//! everything"), warnings are never capped (the honest self-report channel outranks brevity), and
//! ordering is deterministic — severity rank descending, original engine order as the tiebreak — so
//! the same analysis produces byte-identical tool output.

/// Default cap for findings lists. Deliberately small: the default answer is a summary an agent can
/// reason over; the `severity`/`rule`/`limit` tool arguments are the drill-down.
pub const DEFAULT_FINDINGS_LIMIT: usize = 50;
/// Default cap for cross-layer edge lists (edges are small rows; agents usually want them all).
pub const DEFAULT_EDGES_LIMIT: usize = 200;
/// Cap per bucket for `cross_repo`'s `bucketKeys` distinct-key lists (see `bucket_keys`).
pub const DEFAULT_BUCKET_KEYS_LIMIT: usize = 20;
/// Upper bound for a caller-supplied `limit` — keeps a single tool reply bounded no matter what.
pub const MAX_LIMIT: usize = 1000;

/// Caller-facing filters for a findings list, straight from tool arguments.
#[derive(Debug)]
pub struct FindingFilters {
    /// Minimum severity (`"info"` < `"warning"` < `"critical"`). `None` = no severity filter.
    pub min_severity: Option<String>,
    /// Exact rule id to keep. `None` = all rules.
    pub rule: Option<String>,
    /// List cap. `None` = `DEFAULT_FINDINGS_LIMIT`.
    pub limit: Option<usize>,
}

impl FindingFilters {
    /// Parses the shared `severity`/`rule`/`limit` tool arguments. Unknown severity values are a
    /// caller error and must be answered, not guessed around — the error names the valid values.
    pub fn from_args(args: Option<&serde_json::Value>) -> Result<Self, String> {
        let min_severity = args
            .and_then(|a| a.get("severity"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if let Some(s) = &min_severity {
            if severity_rank(s) == 0 {
                return Err(format!(
                    "unknown severity {s:?} — valid values: \"critical\", \"warning\", \"info\""
                ));
            }
        }
        let rule = args
            .and_then(|a| a.get("rule"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let limit = args
            .and_then(|a| a.get("limit"))
            .and_then(|v| v.as_u64())
            .map(|n| (n as usize).min(MAX_LIMIT));
        Ok(FindingFilters {
            min_severity,
            rule,
            limit,
        })
    }
}

/// `critical` > `warning` > `info` > anything else (unknown severities rank 0: shown last unfiltered,
/// excluded by any explicit severity filter — same "never trips a gate it can't name" stance as the
/// JS CLI's severityRank).
fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 3,
        "warning" => 2,
        "info" => 1,
        _ => 0,
    }
}

/// Shapes a findings array into `{total, bySeverity, byRule, shown, truncated?}`.
/// Counts are ALWAYS over the full set (the summary never shrinks with the filter); `shown` is the
/// filtered, severity-desc-sorted, capped list; `truncated` appears ONLY when `shown` is incomplete.
pub fn shape_findings(
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
    // Severity-desc, original engine order as the stable tiebreak — deterministic.
    matching.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));

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
    if total_matching > limit {
        out["truncated"] = truncation(limit, total_matching);
    }
    out
}

/// Shapes a plain list (edges, ...) into `(shown, truncated?)` with the same disclosure contract.
pub fn shape_list(
    items: &[serde_json::Value],
    limit: usize,
) -> (Vec<serde_json::Value>, Option<serde_json::Value>) {
    let shown: Vec<serde_json::Value> = items.iter().take(limit).cloned().collect();
    let truncated = (items.len() > limit).then(|| truncation(limit, items.len()));
    (shown, truncated)
}

fn truncation(shown: usize, total_matching: usize) -> serde_json::Value {
    serde_json::json!({
        "shown": shown,
        "totalMatching": total_matching,
        "hint": "narrow with the severity/rule tool arguments or raise limit",
    })
}

/// The five non-edge cross-layer buckets, in engine (`CrossLayerResult`) field order.
const KEY_BUCKETS: [&str; 5] = [
    "unconsumedProvides",
    "unprovidedConsumes",
    "unresolvedConsumes",
    "externalConsumes",
    "ambiguousConsumes",
];

/// `cross_repo`'s `bucketKeys`: per non-edge bucket, up to `DEFAULT_BUCKET_KEYS_LIMIT` DISTINCT keys
/// (deduped, engine order preserved) so an agent can see WHICH keys sit in a bucket instead of only
/// how many. An unresolved consume (`key: null`) contributes its `raw` expression when recorded —
/// nothing otherwise (never guessed). Returns `(bucketKeys, bucketKeysTruncated?)`; the second is
/// `Some({bucket: remainingDistinctCount})` only when a bucket's distinct-key list was capped —
/// the same explicit-truncation-disclosure stance as `shape_list`, in a per-bucket remainder shape.
pub fn bucket_keys(
    cross_layer: &serde_json::Value,
) -> (serde_json::Value, Option<serde_json::Value>) {
    let mut keys_out = serde_json::Map::new();
    let mut truncated = serde_json::Map::new();
    for bucket in KEY_BUCKETS {
        let mut seen = std::collections::HashSet::new();
        let mut distinct: Vec<&str> = Vec::new();
        for item in cross_layer[bucket]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let key = item
                .get("key")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("raw").and_then(|v| v.as_str()));
            if let Some(k) = key {
                if seen.insert(k) {
                    distinct.push(k);
                }
            }
        }
        if distinct.len() > DEFAULT_BUCKET_KEYS_LIMIT {
            truncated.insert(
                bucket.to_string(),
                serde_json::json!(distinct.len() - DEFAULT_BUCKET_KEYS_LIMIT),
            );
            distinct.truncate(DEFAULT_BUCKET_KEYS_LIMIT);
        }
        keys_out.insert(bucket.to_string(), serde_json::json!(distinct));
    }
    (
        serde_json::Value::Object(keys_out),
        (!truncated.is_empty()).then_some(serde_json::Value::Object(truncated)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(rule: &str, severity: &str, idx: usize) -> serde_json::Value {
        serde_json::json!({ "ruleId": rule, "severity": severity, "path": format!("f{idx}.ts") })
    }

    #[test]
    fn counts_stay_full_while_filter_narrows_shown() {
        let findings = vec![
            finding("a", "info", 0),
            finding("b", "critical", 1),
            finding("a", "warning", 2),
        ];
        let filters = FindingFilters {
            min_severity: Some("warning".into()),
            rule: None,
            limit: None,
        };
        let shaped = shape_findings(&findings, &filters);
        assert_eq!(shaped["total"], 3);
        assert_eq!(shaped["bySeverity"]["info"], 1); // full-set counts, not filtered
        let shown = shaped["shown"].as_array().unwrap();
        assert_eq!(shown.len(), 2);
        // severity-desc ordering: critical before warning
        assert_eq!(shown[0]["severity"], "critical");
        assert!(shaped.get("truncated").is_none()); // complete list => no truncation key
    }

    #[test]
    fn truncation_is_disclosed_never_silent() {
        let findings: Vec<_> = (0..5).map(|i| finding("r", "info", i)).collect();
        let filters = FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(2),
        };
        let shaped = shape_findings(&findings, &filters);
        assert_eq!(shaped["shown"].as_array().unwrap().len(), 2);
        assert_eq!(shaped["truncated"]["shown"], 2);
        assert_eq!(shaped["truncated"]["totalMatching"], 5);
        assert!(shaped["truncated"]["hint"]
            .as_str()
            .unwrap()
            .contains("limit"));
    }

    #[test]
    fn deterministic_order_same_input_same_output() {
        let findings = vec![
            finding("a", "warning", 0),
            finding("b", "warning", 1),
            finding("c", "critical", 2),
        ];
        let filters = FindingFilters {
            min_severity: None,
            rule: None,
            limit: None,
        };
        let one = serde_json::to_string(&shape_findings(&findings, &filters)).unwrap();
        let two = serde_json::to_string(&shape_findings(&findings, &filters)).unwrap();
        assert_eq!(one, two);
        let shaped = shape_findings(&findings, &filters);
        let shown = shaped["shown"].as_array().unwrap();
        // critical first, then the two warnings in original order (stable tiebreak).
        assert_eq!(shown[0]["ruleId"], "c");
        assert_eq!(shown[1]["ruleId"], "a");
        assert_eq!(shown[2]["ruleId"], "b");
    }

    #[test]
    fn unknown_severity_argument_is_a_named_error() {
        let args = serde_json::json!({ "severity": "sev-nope" });
        let err = FindingFilters::from_args(Some(&args)).unwrap_err();
        assert!(err.contains("sev-nope"));
        assert!(err.contains("critical"));
    }

    #[test]
    fn rule_filter_is_exact() {
        let findings = vec![finding("a", "info", 0), finding("ab", "info", 1)];
        let filters = FindingFilters {
            min_severity: None,
            rule: Some("a".into()),
            limit: None,
        };
        let shaped = shape_findings(&findings, &filters);
        assert_eq!(shaped["shown"].as_array().unwrap().len(), 1);
        assert_eq!(shaped["shown"][0]["ruleId"], "a");
    }
}
