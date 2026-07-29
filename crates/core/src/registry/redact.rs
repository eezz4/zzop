//! Evidence-path redaction — the half of `exclude` that keeps a finding but removes the excluded path
//! from it.
//!
//! ## Why one key with two behaviours
//! `exclude` used to read a finding's ANCHOR alone, so the contract it enforced was "do not anchor a
//! finding here" while every document promised "do not name this path to me". A relational finding
//! anchored on the consume side sailed straight through the provide side's `exclude` and printed that
//! path in its message anyway.
//!
//! The fix is not a second config key. A path plays one of two ROLES in a finding and the role decides
//! the treatment, which is a thing the engine can see without being told:
//! - **anchor** (`Finding::file`) — this path is the SUBJECT. Excluding it drops the finding whole, which
//!   is what `exclude` already did and stays unchanged.
//! - **evidence** ([`Finding::evidence_paths`]) — this path appears in someone ELSE'S problem. Excluding
//!   it keeps the finding and removes the path from what gets printed.
//!
//! "My folder is the subject of the problem, I don't look at it. My folder is evidence in someone else's
//! problem, I see the problem but my paths are not named." Splitting that into `exclude` +
//! `partialExclude` was considered and rejected: two knobs for one intent, and the author would have to
//! know which role their path plays in a finding they have not seen yet.
//!
//! ## What redaction does, exactly
//! Every occurrence of the path is replaced by [`REDACTED`] — in `message`, in every string anywhere
//! inside `data` (including inside longer strings, so a `"src/a.ts:12"` site entry redacts too), and the
//! entry is dropped from `evidence_paths`. Substring replacement rather than whole-value replacement is
//! deliberate: a path that survives inside a composite string is a path that was not excluded.
//!
//! Deleting the evidence outright was rejected — a `data` key vanishing changes the payload's SHAPE, and
//! consumers read those keys positionally. A marker keeps the shape and says a filter ran, which is the
//! same "replacement, not suppression" stance the aggregate rules take.

use crate::finding::Finding;

use super::config::{global_exclude_matches_path, suppression_matches_path, RuleConfig};

/// What an excluded evidence path is replaced with. Deliberately not an empty string: the reader must be
/// able to tell "this run was filtered here" from "this finding never named a second place".
pub const REDACTED: &str = "<excluded>";

/// Redacts every evidence path of `finding` that the config excludes, in place. The anchor is NOT
/// consulted — a finding whose anchor is excluded never reaches here (`merge_findings` drops it first).
pub(super) fn redact_excluded_evidence(config: &RuleConfig, finding: &mut Finding) {
    if finding.evidence_paths.is_empty() {
        return;
    }
    let rule = finding.rule_id.clone();
    let excluded: Vec<String> = finding
        .evidence_paths
        .iter()
        .filter(|p| evidence_is_excluded(config, &rule, p))
        .cloned()
        .collect();
    if excluded.is_empty() {
        return;
    }
    for path in &excluded {
        finding.message = finding.message.replace(path.as_str(), REDACTED);
        if let Some(data) = finding.data.as_mut() {
            redact_in_json(data, path);
        }
    }
    finding.evidence_paths.retain(|p| !excluded.contains(p));
}

/// Whether `path`, appearing as EVIDENCE in a finding of `rule`, is excluded. Both channels apply, each
/// with its own scope: a top-level `exclude` is rule-agnostic, a `suppressions` entry only speaks for the
/// rule it names — the identical scoping `is_suppressed` gives the anchor, so one config means one thing
/// whichever role a path turns out to play.
fn evidence_is_excluded(config: &RuleConfig, rule: &str, path: &str) -> bool {
    if config
        .global_excludes
        .iter()
        .any(|entry| global_exclude_matches_path(entry, path))
    {
        return true;
    }
    config
        .suppressions
        .iter()
        .any(|entry| entry.rule == rule && suppression_matches_path(entry, path))
}

/// Replaces `path` with [`REDACTED`] inside every string value reachable in `value`. Object KEYS are left
/// alone: a key is part of the payload's schema, and rewriting one would produce a `data` shape no
/// consumer can read.
fn redact_in_json(value: &mut serde_json::Value, path: &str) {
    match value {
        serde_json::Value::String(s) => {
            if s.contains(path) {
                *s = s.replace(path, REDACTED);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_in_json(item, path);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                redact_in_json(v, path);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
