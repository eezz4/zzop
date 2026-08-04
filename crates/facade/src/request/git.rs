//! Git-window request shapes — the wire form of the config's `git` object.
//!
//! Split out of `request.rs` on 2026-08-02 to stay under the repo's per-file line cap, on the same seam
//! `parsers.rs` uses: these three types are the only ones in the request surface that describe the
//! HISTORY WINDOW a run reads, rather than what to analyze or which judgments to make.

use serde::Deserialize;

/// `AnalyzeRequest::git`'s payload — mirrors `zzop_engine::GitOptions` field-for-field, as JSON input.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GitOptionsRequest {
    pub since: Option<String>,
    pub recent_days: Option<u32>,
    /// Custom commit-type classifier table — the wire exposure of config `git.commitTypePatterns`.
    /// REPLACES `zzop_metrics::default_commit_type_patterns()` entirely when present and non-empty (match
    /// order = array order); absent or an empty array falls back to the default table. See
    /// `zzop_engine::GitOptions::commit_type_patterns`'s doc for the full contract, including how an
    /// invalid regex is handled (skipped, surfaced as a `warnings` entry, never a panic).
    pub commit_type_patterns: Option<Vec<CommitTypePatternRequest>>,
    /// DECLARED subject-pattern table — the wire exposure of config `git.commitSubjectPatterns`.
    /// Absent or empty means NO commit gets a label: this axis has no default table to fall back to,
    /// deliberately (see `zzop_engine::GitOptions::commit_subject_patterns`). Independent of
    /// `commit_type_patterns` in every way — different output field (`labels`, not `tags`), all
    /// matches kept rather than first-match-wins, and the pattern is compiled exactly as written with
    /// no `(?i)` injected.
    pub commit_subject_patterns: Option<Vec<CommitSubjectPatternRequest>>,
}

/// One `git.commitTypePatterns` config-file entry: `{ pattern: <regex>, tag: <TAG> }`. A dedicated struct
/// (rather than accepting a raw 2-element JSON array over the wire) keeps the shape self-describing for a
/// config-file author; `build_engine_config` flattens the list into the `(String, String)` tuple pairs
/// `zzop_engine::GitOptions::commit_type_patterns` / `zzop_git::CollectOptions::commit_type_patterns` use
/// internally.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitTypePatternRequest {
    pub pattern: String,
    pub tag: String,
}

/// One `git.commitSubjectPatterns` config-file entry: `{ pattern: <regex>, label: <string> }`. Same
/// self-describing-struct rationale as `CommitTypePatternRequest` above; the field is `label` rather
/// than `tag` because the two axes must stay tellable apart at the config surface — a `tag` feeds the
/// commit-TYPE vocabulary (and per-file `tagCounts`), a `label` is whatever the author declared it to
/// mean and rides on `CommitFileSet::labels` alone.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSubjectPatternRequest {
    pub pattern: String,
    pub label: String,
}
