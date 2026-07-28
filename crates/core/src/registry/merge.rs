//! Finding merge — the one deterministically ordered list every rule source's findings flow into.
//! The sort order here is a determinism contract (worst-first, then file order) — do not edit.

use crate::{finding::Finding, Severity};

use super::config::{apply_severity_override, is_suppressed, RuleConfig};

/// Severity sort rank: critical first, then warning, then info (the same order used for ranking
/// recommendation groups in `recommendations.rs`). The file/line/rule-id tie-breakers below give a
/// deterministic, human-scannable "worst-first, then file order" report.
fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Critical => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

/// Merges findings from every rule source (native analyses, DSL packs) into one
/// deterministically ordered list: drops suppressed findings (`is_suppressed`), applies severity overrides
/// (`apply_severity_override`), then sorts by severity (critical < warning < info), then file, then line,
/// then rule id (see `severity_rank` doc for the sort's provenance/design-call note). Pure — no I/O, no
/// dependency on which layer produced a given `Vec<Finding>`.
///
/// **No folding happens here, and it is not an omission** (2026-07-26 judgment, recorded where the
/// determinism contract lives so the question is not re-opened blind). A general "collapse repeated
/// same-shape findings" step is structurally impossible at this layer and would be lossy if forced:
/// `Finding` carries `rule_id`/`severity`/`file`/`line`/`message`/`data`, `data` is opaque JSON this crate
/// never interprets, and no rule metadata reaches here at all — so "same shape" has no rule-agnostic
/// definition, and the only fields a merge-layer fold COULD group on (`file`, `line`) are precisely the
/// ones that make a finding actionable. Folding therefore lives in the rules, where the meaning of "the
/// same cause" is known: `MIN_PREFIX_DRIFT_GROUP` (`rules-cross-layer/prefix_drift.rs:28`),
/// `MIN_FOREIGN_UNPROVIDED_GROUP` (`rules-http/unprovided_consume.rs:119`), and
/// `MAX_LISTED_PER_SOURCE` (`rules-cross-layer/unconsumed_endpoint.rs`) are the three that exist. A new
/// measured volume case gets a fourth one in ITS rule — not a fifth concept here.
pub fn merge_findings(sources: Vec<Vec<Finding>>, config: &RuleConfig) -> Vec<Finding> {
    let mut merged: Vec<Finding> = sources
        .into_iter()
        .flatten()
        .filter(|f| !is_suppressed(config, &f.rule_id, Some(f.file.as_str())))
        .map(|f| apply_severity_override(config, f))
        .collect();
    merged.sort_by(|a, b| {
        severity_rank(a.severity)
            .cmp(&severity_rank(b.severity))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });
    merged
}
