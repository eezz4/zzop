//! Coupling score — linear penalty from avg fan-out plus a circular-reference penalty. Fan-out above a
//! "knee" value degrades the score at a fixed slope; a circular-import count adds a separate, capped
//! penalty on top. This is `scores::coupling` (a 0-100 health score) — distinct from the crate-root
//! `crate::coupling` module (the co-change `CouplingMap`); they share a name only by coincidence.

use super::config::ScoresConfig;
use super::types::CouplingScore;
use zzop_core::FileNode;

pub fn compute_coupling(
    nodes: &[FileNode],
    circular_count: usize,
    cfg: &ScoresConfig,
) -> CouplingScore {
    let live: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| n.loc > 0 && n.fan_out > 0)
        .collect();
    if live.is_empty() {
        return CouplingScore {
            score: 100.0,
            avg_fan_out: 0.0,
            max_fan_out: 0.0,
            circular_count: circular_count as u32,
        };
    }
    let t = &cfg.thresholds.coupling;
    let fan_outs: Vec<u32> = live.iter().map(|n| n.fan_out).collect();
    let avg = fan_outs.iter().map(|&f| f as f64).sum::<f64>() / live.len() as f64;
    // Non-empty per the `live.is_empty()` guard above, so this never mirrors JS's `Math.max() === -Infinity`.
    let max = fan_outs.iter().copied().max().unwrap_or(0);
    let fan_out_score = (100.0 - (avg - t.fan_out_knee).max(0.0) * t.fan_out_slope).max(0.0);
    let circular_penalty = (circular_count as f64 * t.circular_weight).min(t.circular_cap);
    let score = (fan_out_score - circular_penalty).max(0.0);
    CouplingScore {
        score: score.round(),
        avg_fan_out: (avg * 10.0).round() / 10.0,
        max_fan_out: max as f64,
        circular_count: circular_count as u32,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the empty/no-eligible-files baseline, the fan-out knee and penalty slope, the circular-penalty
    //! cap, and the score floor at 0 — all against `ScoresConfig::default()`.
    use super::*;

    fn node(fan_out: u32) -> FileNode {
        FileNode {
            id: "x".to_string(),
            path: "x".to_string(),
            change_count: 0,
            churn: 0,
            last_modified: None,
            author_count: 1,
            loc: 10,
            tag_counts: Default::default(),
            fan_in: 0,
            fan_out,
            total_connections: 0,
            risk_score: 0.0,
            ..Default::default()
        }
    }

    #[test]
    fn no_files_with_fan_out_gt_0_score_100() {
        let r = compute_coupling(&[node(0)], 0, &ScoresConfig::default());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.avg_fan_out, 0.0);
        assert_eq!(r.max_fan_out, 0.0);
        assert_eq!(r.circular_count, 0);
    }

    #[test]
    fn empty_input_score_100() {
        assert_eq!(
            compute_coupling(&[], 0, &ScoresConfig::default()).score,
            100.0
        );
    }

    #[test]
    fn avg_fan_out_le_5_no_penalty_score_100() {
        // live fanOuts [4, 6] avg 5 -> fanOutScore = 100 - max(0, 5-5)*10 = 100, no circular
        let nodes = [node(4), node(6)];
        let r = compute_coupling(&nodes, 0, &ScoresConfig::default());
        assert_eq!(r.avg_fan_out, 5.0);
        assert_eq!(r.max_fan_out, 6.0);
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn avg_fan_out_8_fan_out_score_70() {
        // fanOuts [6, 10] avg 8 -> 100 - (8-5)*10 = 70, no circular
        let nodes = [node(6), node(10)];
        let r = compute_coupling(&nodes, 0, &ScoresConfig::default());
        assert_eq!(r.avg_fan_out, 8.0);
        assert_eq!(r.max_fan_out, 10.0);
        assert_eq!(r.score, 70.0);
    }

    #[test]
    fn circular_penalty_applied_and_capped_at_30() {
        // avg 8 -> fanOutScore 70; circular 10 -> penalty min(30, 50) = 30 -> 70 - 30 = 40
        let nodes = [node(6), node(10)];
        let r = compute_coupling(&nodes, 10, &ScoresConfig::default());
        assert_eq!(r.score, 40.0);
        assert_eq!(r.circular_count, 10);
    }

    #[test]
    fn score_floors_at_0() {
        // fanOuts [20] avg 20 -> 100 - (20-5)*10 = 100 - 150 = -50 -> max(0,..) = 0
        let r = compute_coupling(&[node(20)], 0, &ScoresConfig::default());
        assert_eq!(r.score, 0.0);
    }
}
