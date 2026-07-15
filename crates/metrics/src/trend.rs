//! Builds time-series data points for the top-N files across multiple SHA snapshots.
//!
//! Input snapshots must be newest-first. Series covers only the top-N files by risk score in the
//! current (first) snapshot; missing values in older snapshots are `None`.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Default top-N files by risk score from the current snapshot. Rust has no optional function
/// arguments, so callers that want this default pass the constant explicitly.
pub const TOP_N: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendSnapshot {
    pub sha: String,
    pub generated_at: String,
    /// path -> risk score. `BTreeMap` since the top-N selection re-sorts by value anyway; ties fall back to
    /// key-ascending order, which keeps the result deterministic.
    pub files: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendPoint {
    pub sha: String,
    pub generated_at: String,
    /// `None` = file did not exist at this snapshot.
    pub risk: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendSeries {
    pub path: String,
    pub points: Vec<TrendPoint>,
}

/// `snapshots` must be newest-first (`snapshots[0]` = current). `top_n` = how many files (by current-snapshot risk
/// score, descending) get a series.
pub fn build_trend_series(snapshots: &[TrendSnapshot], top_n: usize) -> Vec<TrendSeries> {
    if snapshots.is_empty() {
        return vec![];
    }
    let current = &snapshots[0];
    // oldest-first -- natural left(past)->right(present) order for charts.
    let ordered: Vec<&TrendSnapshot> = snapshots.iter().rev().collect();

    let mut ranked: Vec<(&String, &f64)> = current.files.iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(Ordering::Equal));
    let paths: Vec<String> = ranked
        .into_iter()
        .take(top_n)
        .map(|(p, _)| p.clone())
        .collect();

    paths
        .into_iter()
        .map(|path| {
            let points = ordered
                .iter()
                .map(|s| TrendPoint {
                    sha: s.sha.clone(),
                    generated_at: s.generated_at.clone(),
                    risk: s.files.get(&path).copied(),
                })
                .collect();
            TrendSeries { path, points }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Covers empty-input handling, top-N selection by current-snapshot risk score with past values
    //! filled in time-ascending order, `None` for files absent in older snapshots, the default top-N
    //! constant, and single-snapshot series.
    use super::*;

    fn snapshot(sha: &str, generated_at: &str, files: &[(&str, f64)]) -> TrendSnapshot {
        TrendSnapshot {
            sha: sha.to_string(),
            generated_at: generated_at.to_string(),
            files: files.iter().map(|(p, v)| (p.to_string(), *v)).collect(),
        }
    }

    #[test]
    fn empty_snapshots_empty_series() {
        assert_eq!(build_trend_series(&[], TOP_N), vec![]);
    }

    #[test]
    fn selects_top_n_by_current_snapshot_risk_score_and_fills_past_values() {
        let snapshots = vec![
            snapshot("s3", "t3", &[("hot", 100.0), ("mid", 50.0), ("low", 10.0)]),
            snapshot("s2", "t2", &[("hot", 80.0), ("mid", 45.0), ("low", 20.0)]),
            snapshot("s1", "t1", &[("hot", 60.0), ("mid", 40.0), ("low", 30.0)]),
        ];
        let series = build_trend_series(&snapshots, 2);
        let paths: Vec<&str> = series.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, vec!["hot", "mid"]);
        // time-ascending order (s1 -> s3).
        let hot_risks: Vec<Option<f64>> = series[0].points.iter().map(|p| p.risk).collect();
        assert_eq!(hot_risks, vec![Some(60.0), Some(80.0), Some(100.0)]);
        let mid_risks: Vec<Option<f64>> = series[1].points.iter().map(|p| p.risk).collect();
        assert_eq!(mid_risks, vec![Some(40.0), Some(45.0), Some(50.0)]);
    }

    #[test]
    fn files_absent_in_past_snapshots_produce_null_points() {
        let snapshots = vec![
            snapshot("s2", "t2", &[("newFile", 80.0)]),
            snapshot("s1", "t1", &[("oldOnly", 50.0)]),
        ];
        let series = build_trend_series(&snapshots, TOP_N);
        assert_eq!(series[0].path, "newFile");
        let risks: Vec<Option<f64>> = series[0].points.iter().map(|p| p.risk).collect();
        assert_eq!(risks, vec![None, Some(80.0)]);
    }

    #[test]
    fn default_top_n_20() {
        let file_map: BTreeMap<String, f64> =
            (0..30).map(|i| (format!("f{i}"), i as f64)).collect();
        let snapshots = vec![TrendSnapshot {
            sha: "x".to_string(),
            generated_at: "t".to_string(),
            files: file_map,
        }];
        let series = build_trend_series(&snapshots, TOP_N);
        assert_eq!(series.len(), 20);
        // top 20 = f29 ~ f10 (descending list).
        assert_eq!(series[0].path, "f29");
        assert_eq!(series[19].path, "f10");
    }

    #[test]
    fn single_snapshot_still_returns_series() {
        let snapshots = vec![snapshot("s", "t", &[("a", 10.0)])];
        let series = build_trend_series(&snapshots, TOP_N);
        assert_eq!(
            series,
            vec![TrendSeries {
                path: "a".to_string(),
                points: vec![TrendPoint {
                    sha: "s".to_string(),
                    generated_at: "t".to_string(),
                    risk: Some(10.0),
                }],
            }]
        );
    }
}
