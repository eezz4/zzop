//! Caller-facing findings filters and their validation vocabulary — split from `output/mod.rs` at
//! the filters/shaping seam when the module crossed the per-file line cap. The two halves meet at
//! exactly two symbols: [`FindingFilters`] (what the caller asked for) and [`severity_rank`] (the
//! one severity ordering both filtering and shaping must share, or a filter could accept a severity
//! the sort ranks as unknown).

use super::MAX_LIMIT;

/// Caller-facing filters for a findings list. Two constructors, one validation vocabulary:
/// [`FindingFilters::new`] is WIRE-NEUTRAL (already-parsed values — what the CLI has after argv
/// parsing) and [`FindingFilters::from_args`] reads an MCP `tools/call` `arguments` object. `new` is the
/// core; `from_args` is the JSON front door onto it, so the two can never disagree about what a valid
/// `severity`/`limit` is.
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
    /// WIRE-NEUTRAL constructor: already-parsed values in, the same validation vocabulary applied.
    /// This is what a host that does NOT speak JSON calls — the `zzop` CLI has `severity`/`rule`/`limit`
    /// as argv strings/numbers and must never have to fabricate an MCP `tools/call` object to reach the
    /// shared filters (a host assembling a foreign wire shape to talk to the shared layer is a protocol
    /// leak into a protocol-free crate, and it made the CLI's filter lane look like it needed an MCP
    /// dependency it does not have).
    ///
    /// Same rejections as [`from_args`](Self::from_args), by construction rather than by copy: an
    /// unrecognized `min_severity` is a named error with the valid vocabulary, and a `limit` above
    /// `MAX_LIMIT` is a named range error. `None` means "no filter" for all three — the only way to say
    /// it here, since a wire-neutral caller has no `null` to distinguish.
    ///
    /// `FindingFilters::new(None, None, None)` is the unfiltered default view (what `zzop analyze`
    /// prints today), and it cannot fail — but the signature still returns `Result`, so a caller that
    /// later starts passing real user input is not tempted to `unwrap` a validation away.
    pub fn new(
        min_severity: Option<&str>,
        rule: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Self, String> {
        if let Some(severity) = min_severity {
            if severity_rank(severity) == 0 {
                return Err(unknown_severity_error(&serde_json::Value::String(
                    severity.to_string(),
                )));
            }
        }
        if let Some(n) = limit {
            if n > MAX_LIMIT {
                return Err(limit_range_error(&serde_json::Value::from(n)));
            }
        }
        Ok(FindingFilters {
            min_severity: min_severity.map(str::to_string),
            rule: rule.map(str::to_string),
            limit,
        })
    }

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
        })
    }
}

/// `unknown severity <value> — valid values: ...` — `{v}` (not `{v:?}`) relies on
/// `serde_json::Value`'s `Display` impl serializing exactly like `{:?}` does for a `&str` (both quote
/// a string value), so this message is byte-identical to the pre-existing string-only wording while
/// also covering every other JSON type (a bare `5`, `true`, `[1,2]`, ...) with no special-casing.
pub(super) fn unknown_severity_error(v: &serde_json::Value) -> String {
    format!("unknown severity {v} — valid values: \"critical\", \"warning\", \"info\"")
}

/// Strict `limit` validation: must be a JSON INTEGER (a float literal like `3.7`, or one that merely
/// looks whole like `5.0`, is rejected — `serde_json::Value::as_i64` only succeeds on a value parsed
/// from integer literal syntax) within `[0, MAX_LIMIT]`. `0` is legal — "counts only, no findings
/// listed" is a useful zero-cost query, so the schema's `minimum` matches (see `tools::definitions`).
pub(super) fn parse_limit(v: &serde_json::Value) -> Result<usize, String> {
    match v.as_i64() {
        Some(n) if (0..=MAX_LIMIT as i64).contains(&n) => Ok(n as usize),
        _ => Err(limit_range_error(v)),
    }
}

/// `limit must be an integer between 0 and <MAX> (got <value>)` — shared by [`parse_limit`] (the JSON
/// lane, where the value can also be the wrong TYPE) and `FindingFilters::new`'s range check, so both
/// constructors reject an over-cap limit with the identical message.
pub(super) fn limit_range_error(v: &serde_json::Value) -> String {
    format!("limit must be an integer between 0 and {MAX_LIMIT} (got {v})")
}

/// `critical` > `warning` > `info` > anything else (unknown severities rank 0: shown last unfiltered,
/// excluded by any explicit severity filter — same "never trips a gate it can't name" stance as
/// severityRank).
pub(crate) fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 3,
        "warning" => 2,
        "info" => 1,
        _ => 0,
    }
}
