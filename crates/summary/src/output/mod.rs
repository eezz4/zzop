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
#[cfg(test)]
mod tests;

pub(crate) use bucket_keys::bucket_keys;
pub use bucket_keys::DEFAULT_BUCKET_KEYS_LIMIT;

/// Output verbosity for analyze-shaped replies. `Summary` (the default) is the token-bomb-guarded
/// shaped reply every MCP tool returns today; `Full` additionally emits the raw output fields the
/// summary drops or compacts — the raw `zzop-facade` embedder data lane. STAGED, not yet
/// caller-reachable: every caller constructs `FindingFilters` with `Summary`, so the `Full`
/// branches in the analyze/cross shapers are dead today and current replies stay byte-identical. A
/// later host<->facade parity flip exposes a `verbosity`/`raw` tool argument + `zzop-mcp` CLI flag (and
/// must then update the tool self-descriptions that today promise the raw `ir` block is the
/// direct-facade-embedding lane only). See the crate doc's "Host<->facade parity (staged)" section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
    #[default]
    Summary,
    Full,
}

/// Caller-facing filters for a findings list, straight from tool arguments.
#[derive(Debug)]
pub struct FindingFilters {
    /// Minimum severity (`"info"` < `"warning"` < `"critical"`). `None` = no severity filter.
    pub min_severity: Option<String>,
    /// Exact rule id to keep. `None` = all rules.
    pub rule: Option<String>,
    /// List cap. `None` = `DEFAULT_FINDINGS_LIMIT`.
    pub limit: Option<usize>,
    /// Reply verbosity — [`Verbosity::Summary`] today (see [`Verbosity`]); [`Verbosity::Full`] is
    /// staged, not yet reachable from any tool argument.
    pub verbosity: Verbosity,
}

impl FindingFilters {
    /// Parses the shared `severity`/`rule`/`limit` tool arguments. A live-fire boundary-value round
    /// found every one of these silently ignored the WRONG JSON type instead of rejecting it: a
    /// `severity` NUMBER fell through `as_str()` to "no filter" the same way an absent key would, and a
    /// `limit` of `-1`/`1001`/`999999`/`"50"`/`3.7` all silently behaved as "no cap" (`as_u64()` returns
    /// `None` on a negative, a float, or a string, which the old code then treated as "not provided").
    /// Every rejection below is a NAMED caller error instead — an unknown/wrong-typed `severity` value
    /// and an out-of-range/wrong-typed `limit` value both fail loudly, never silently.
    pub fn from_args(args: Option<&serde_json::Value>) -> Result<Self, String> {
        let min_severity = match args.and_then(|a| a.get("severity")) {
            None | Some(serde_json::Value::Null) => None,
            Some(v) => {
                // A non-string severity (e.g. the NUMBER `5`) must hit the SAME rejection path as an
                // unknown string — `as_str()` returning `None` here is `severity_rank`'s "unranked"
                // case too, so routing both through one check keeps the message and the vocabulary
                // consistent regardless of which way the value was wrong.
                let s = v.as_str();
                if s.map(severity_rank).unwrap_or(0) == 0 {
                    return Err(unknown_severity_error(v));
                }
                s.map(str::to_string)
            }
        };
        let rule = crate::args::optional_string(args, "rule")?.map(str::to_string);
        let limit = match args.and_then(|a| a.get("limit")) {
            None | Some(serde_json::Value::Null) => None,
            Some(v) => Some(parse_limit(v)?),
        };
        Ok(FindingFilters {
            min_severity,
            rule,
            limit,
            // Staged: no tool argument reads this yet, so every parsed filter is `Summary` and the
            // `Full` lane stays dead (see [`Verbosity`]).
            verbosity: Verbosity::Summary,
        })
    }
}

/// The raw `AnalyzeOutputView` fields the `Summary` reply drops or compacts, emitted verbatim only on
/// the staged [`Verbosity::Full`] lane. `health`/`recommendations`/`critical` are the un-compacted
/// originals of the `Summary` reply's compact `architecture` object.
pub(crate) const FULL_ONLY_OUTPUT_FIELDS: &[&str] = &[
    "ir",
    "nodes",
    "scores",
    "seams",
    "folders",
    "layerCoChurn",
    "cache",
    "ruleTimings",
    "health",
    "recommendations",
    "critical",
];

/// Inserts every present [`FULL_ONLY_OUTPUT_FIELDS`] entry from `output_view` into `map` (defensive
/// `.get()` — a field an older engine output lacks is simply not forwarded, never `null`). The staged
/// parity primitive the single-tree and cross-tree shapers both call behind a [`Verbosity::Full`] guard.
pub(crate) fn insert_full_output_fields(
    map: &mut serde_json::Map<String, serde_json::Value>,
    output_view: &serde_json::Value,
) {
    for &field in FULL_ONLY_OUTPUT_FIELDS {
        if let Some(v) = output_view.get(field) {
            map.insert(field.to_string(), v.clone());
        }
    }
}

/// `unknown severity <value> — valid values: ...` — `{v}` (not `{v:?}`) relies on
/// `serde_json::Value`'s `Display` impl serializing exactly like `{:?}` does for a `&str` (both quote
/// a string value), so this message is byte-identical to the pre-existing string-only wording while
/// also covering every other JSON type (a bare `5`, `true`, `[1,2]`, ...) with no special-casing.
fn unknown_severity_error(v: &serde_json::Value) -> String {
    format!("unknown severity {v} — valid values: \"critical\", \"warning\", \"info\"")
}

/// Strict `limit` validation: must be a JSON INTEGER (a float literal like `3.7`, or one that merely
/// looks whole like `5.0`, is rejected — `serde_json::Value::as_i64` only succeeds on a value parsed
/// from integer literal syntax) within `[0, MAX_LIMIT]`. `0` is legal — "counts only, no findings
/// listed" is a useful zero-cost query, so the schema's `minimum` matches (see `tools::definitions`).
fn parse_limit(v: &serde_json::Value) -> Result<usize, String> {
    match v.as_i64() {
        Some(n) if (0..=MAX_LIMIT as i64).contains(&n) => Ok(n as usize),
        _ => Err(format!(
            "limit must be an integer between 0 and {MAX_LIMIT} (got {v})"
        )),
    }
}

/// `critical` > `warning` > `info` > anything else (unknown severities rank 0: shown last unfiltered,
/// excluded by any explicit severity filter — same "never trips a gate it can't name" stance as
/// severityRank).
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
    // Zero-match rule-filter disclosure: `shown: []` from a real rule with zero findings this run is
    // indistinguishable from `shown: []` from a TYPO'd/nonexistent rule id — both look identical on the
    // wire. When a `rule` filter is present and matched nothing, cross-check it against `byRule` (built
    // from the FULL, unfiltered set above): a rule id absent from `byRule` never fired at all this run,
    // so the filter is almost certainly wrong rather than merely quiet. Deterministic and additive-only
    // (never fires when the filter matched >=1 finding, never touches `shown`/`truncated`).
    if let Some(rule) = &filters.rule {
        if total_matching == 0 && !by_rule.contains_key(rule.as_str()) {
            out["note"] = serde_json::Value::String(format!(
                "rule filter '{rule}' matched no findings and is not among this run's fired rule ids — \
                 check the id (byRule lists what fired; the `rule-catalog` contract lists all ids — \
                 read it via the zzop://contract/rule-catalog resource or `zzop-mcp contract rule-catalog`)"
            ));
        }
    }
    out
}

/// Shapes a plain list (edges, ...) into `(shown, truncated?)` with the same disclosure contract.
pub(crate) fn shape_list(
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
