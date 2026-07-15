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
}
