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

/// Merges findings from every rule source (native analyses, DSL packs, JS quick-rules) into one
/// deterministically ordered list: drops suppressed findings (`is_suppressed`), applies severity overrides
/// (`apply_severity_override`), then sorts by severity (critical < warning < info), then file, then line,
/// then rule id (see `severity_rank` doc for the sort's provenance/design-call note). Pure — no I/O, no
/// dependency on which layer produced a given `Vec<Finding>`.
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
