//! `CommitFileSet` — one commit's touched-file set (shared IR). Produced by `zzop_git`, consumed by
//! `zzop_engine`; the co-change coupling computation lives in `zzop_metrics::coupling`. Shared IR types
//! stay in core even when their downstream computation lives in a dedicated crate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitFileSet {
    pub sha: String,
    pub files: Vec<String>,
    /// `[TAG]` tokens extracted from the commit message (e.g. \["FIX", "REFACTOR"\]); used for line hotspot join.
    pub tags: Vec<String>,
    /// ISO commit date — used to report the analyzed git window (since/first/last).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The commit subject as the collector received it: `zzop_git` never shortens, normalizes or
    /// reconstructs it — no truncation, no case folding, no re-assembly from `tags`. It is NOT
    /// guaranteed byte-identical to what `git log %s` emitted, because a decode boundary sits in
    /// between: `zzop_git`'s git-process layer turns stdout into a `String` with
    /// `String::from_utf8_lossy`, which replaces every non-UTF-8 byte with U+FFFD. Git re-encodes a
    /// message only when the commit object carries an `encoding` header, so a legacy latin-1 /
    /// Shift-JIS subject written without one arrives here with its high bytes already replaced.
    /// `tags` above is lossy by construction (it holds only the `[TAG]` tokens or a single
    /// classifier verdict), so before this field existed the subject was read during collection and
    /// then thrown away, with no way to recover it. `None` only when the commit genuinely has an
    /// empty subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Labels from the caller-DECLARED subject-pattern table (`git.commitSubjectPatterns`), in
    /// declaration order, first occurrence kept. ALWAYS EMPTY when the caller declared no table:
    /// this axis has no built-in vocabulary at all — not a default table, not a fallback — because
    /// what a "revert"/"ticket"/"hotfix" subject looks like is a per-project convention the engine
    /// would have to GUESS. `tags` (the commit-TYPE axis, which does have a default vocabulary a
    /// caller supplies) stays a separate field; the two are never merged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}
