//! FileNode — per-file analysis node.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// File-state classification (churn x recency matrix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    Volatile,
    Settled,
    Stable,
    Fresh,
}

/// `#[serde(rename_all = "camelCase")]`: output-only (reaches `AnalyzeOutputView::nodes`), same
/// casing-unification rationale as `Finding` — see `packages/napi/src/api.rs`'s `AnalyzeOutputView` doc.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    /// Normalized current path (canonical id).
    pub id: String,
    pub path: String,
    pub change_count: u32,
    pub churn: u32,
    pub last_modified: Option<String>,
    pub author_count: u32,
    /// Lines of code.
    pub loc: u32,
    /// `HashMap` iteration order is hasher-randomized per process — `serialize_with` sorts keys so
    /// `tagCounts` serializes byte-deterministically across runs (see `crate::serde_util::sorted_map`'s
    /// doc). Deserialize is untouched.
    #[serde(serialize_with = "crate::serde_util::sorted_map")]
    pub tag_counts: HashMap<String, u32>,
    pub fan_in: u32,
    pub fan_out: u32,
    pub total_connections: u32,
    pub risk_score: f64,
    /// hotspot = changeCount x loc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotspot_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    /// Churn within the last N days (separates aging signal). Used for rename-only commit correction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_churn: Option<u32>,
    /// Change count over the same period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_change_count: Option<u32>,
    /// author email -> commit count for this file. Used for precise knowledge-silo / bus-factor calculation.
    /// See `tag_counts`'s doc — same determinism fix, `Option`-wrapped variant.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::serde_util::sorted_map_option"
    )]
    pub author_commits: Option<HashMap<String, u32>>,
    /// (file, author) commit count within the last N days. Used for recent vs all-time shift comparison.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::serde_util::sorted_map_option"
    )]
    pub recent_author_commits: Option<HashMap<String, u32>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RiskWeights {
    pub change_count: f64,
    pub churn: f64,
    pub total_connections: f64,
}

pub const DEFAULT_WEIGHTS: RiskWeights = RiskWeights {
    change_count: 0.4,
    churn: 0.2,
    total_connections: 0.4,
};

/// The metrics that feed the risk score.
pub struct RiskInput {
    pub change_count: u32,
    pub churn: u32,
    pub total_connections: u32,
}

/// risk_score = w_c*change + w_h*churn + w_d*connections.
pub fn calc_risk_score(input: &RiskInput, w: &RiskWeights) -> f64 {
    input.change_count as f64 * w.change_count
        + input.churn as f64 * w.churn
        + input.total_connections as f64 * w.total_connections
}

/// Files modified within this many days are considered "recent" (default 30).
pub const DEFAULT_RECENT_THRESHOLD_DAYS: f64 = 30.0;
const MS_PER_DAY: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

/// Median churn over the set, excluding churn=0 files.
pub fn compute_median_churn(churns: &[u32]) -> f64 {
    let mut filtered: Vec<u32> = churns.iter().copied().filter(|&c| c > 0).collect();
    if filtered.is_empty() {
        return 0.0;
    }
    filtered.sort_unstable();
    let n = filtered.len();
    let mid = n / 2;
    if n.is_multiple_of(2) {
        (filtered[mid - 1] as f64 + filtered[mid] as f64) / 2.0
    } else {
        filtered[mid] as f64
    }
}

/// Classify a file's lifecycle by churn x recency.
/// `modified_ms` = last-modified epoch ms (None = infinitely old); date-string parsing is the caller's job.
/// `recent_churn` = Some to make rename-only commits (churn 0) not count as a recent touch.
pub fn classify_lifecycle(
    churn: u32,
    modified_ms: Option<i64>,
    median_churn: f64,
    now_ms: i64,
    recent_threshold_days: f64,
    recent_churn: Option<u32>,
) -> Lifecycle {
    let high_churn = churn as f64 > median_churn;
    let age_days = match modified_ms {
        Some(ms) if ms > 0 => (now_ms - ms) as f64 / MS_PER_DAY,
        _ => f64::INFINITY,
    };
    let recent_touched = match recent_churn {
        None => age_days <= recent_threshold_days,
        Some(rc) => rc > 0 && age_days <= recent_threshold_days,
    };
    match (high_churn, recent_touched) {
        (true, true) => Lifecycle::Volatile,
        (true, false) => Lifecycle::Settled,
        (false, true) => Lifecycle::Fresh,
        (false, false) => Lifecycle::Stable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_sum_with_default_weights() {
        let r = calc_risk_score(
            &RiskInput {
                change_count: 10,
                churn: 20,
                total_connections: 5,
            },
            &DEFAULT_WEIGHTS,
        );
        // 10*0.4 + 20*0.2 + 5*0.4 = 10.0
        assert!((r - 10.0).abs() < 1e-9);
    }

    #[test]
    fn zero_input_is_zero() {
        let r = calc_risk_score(
            &RiskInput {
                change_count: 0,
                churn: 0,
                total_connections: 0,
            },
            &DEFAULT_WEIGHTS,
        );
        assert_eq!(r, 0.0);
    }

    // --- classifyLifecycle / computeMedianChurn ---
    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    const NOW: i64 = 1_000_000_000_000;

    fn classify(churn: u32, days_ago: Option<i64>) -> Lifecycle {
        let modified = days_ago.map(|d| NOW - d * DAY_MS);
        classify_lifecycle(
            churn,
            modified,
            50.0,
            NOW,
            DEFAULT_RECENT_THRESHOLD_DAYS,
            None,
        )
    }

    #[test]
    fn median_churn_excludes_zeros() {
        assert_eq!(compute_median_churn(&[0, 10, 20, 30, 40]), 25.0);
    }

    #[test]
    fn median_churn_all_zero_is_zero() {
        assert_eq!(compute_median_churn(&[0, 0, 0]), 0.0);
    }

    #[test]
    fn lifecycle_quadrants() {
        assert_eq!(classify(100, Some(5)), Lifecycle::Volatile); // high + recent
        assert_eq!(classify(100, Some(90)), Lifecycle::Settled); // high + old
        assert_eq!(classify(10, Some(5)), Lifecycle::Fresh); // low + recent
        assert_eq!(classify(10, Some(90)), Lifecycle::Stable); // low + old
        assert_eq!(classify(10, None), Lifecycle::Stable); // null modified = infinitely old
    }
}
