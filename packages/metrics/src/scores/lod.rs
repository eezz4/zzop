//! Law of Demeter density score — lower means fewer chain-coupling violations (`a.b.c`+ property-access chains
//! that reach through an object's internals instead of talking to it directly). Scale: avg 10 violations/file -> 0,
//! avg 0 -> 100.

use std::collections::HashMap;

use crate::scores::config::ScoresConfig;
use crate::scores::types::{LodFileSummary, LodScore};
use zzop_core::FileNode;

/// Max detail rows returned.
const MAX_DETAIL_ITEMS: usize = 50;
/// The 0-100 score scale.
const PERCENT: f64 = 100.0;

/// A single `a.b.c`+ property-access chain found in a file.
/// `chain` is the full dotted chain text, `depth` is its `PropertyAccess` count, `line` is its source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LodChain {
    pub chain: String,
    pub depth: u32,
    pub line: u32,
}

/// `avg_count = total_violations / file_count` over files with `loc > 0`; `score = max(0, round((1 - avg_count /
/// count_cap) * 100))`. A path in `lod_by_file` but absent from `nodes` falls back to `loc = 1`.
pub fn compute_lod(
    nodes: &[FileNode],
    lod_by_file: &HashMap<String, Vec<LodChain>>,
    cfg: &ScoresConfig,
) -> LodScore {
    let loc_by_path: HashMap<&str, u32> = nodes.iter().map(|n| (n.path.as_str(), n.loc)).collect();

    let mut total_violations: u32 = 0;
    let mut summaries: Vec<LodFileSummary> = Vec::new();

    for (path, chains) in lod_by_file {
        let count = chains.len() as u32;
        let max_depth = chains.iter().map(|c| c.depth).max().unwrap_or(0);
        let loc = loc_by_path.get(path.as_str()).copied().unwrap_or(1);
        total_violations += count;
        summaries.push(LodFileSummary {
            path: path.clone(),
            count,
            max_depth,
            loc,
            density: f64::from(count) / f64::from(loc.max(1)),
        });
    }

    // Deterministic tie-break by path — the source is a HashMap, which has no defined iteration order.
    summaries.sort_by(|a, b| {
        b.density
            .partial_cmp(&a.density)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    let file_count = nodes.iter().filter(|n| n.loc > 0).count();
    let avg_count = if file_count > 0 {
        f64::from(total_violations) / file_count as f64
    } else {
        0.0
    };
    let count_cap = cfg.thresholds.lod.count_cap;
    let score = ((1.0 - avg_count / count_cap) * PERCENT).round().max(0.0);

    summaries.truncate(MAX_DETAIL_ITEMS);

    LodScore {
        score,
        total_violations,
        violations: summaries,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the no-chains baseline, per-file chain/depth/density summarization, the score floor at the
    //! violations-per-file cap, sorting violations by density descending, and the loc=1 fallback for a
    //! path present in `lod_by_file` but absent from `nodes`.
    use super::*;

    fn node(path: &str, loc: u32) -> FileNode {
        FileNode {
            id: path.to_string(),
            path: path.to_string(),
            change_count: 0,
            churn: 0,
            last_modified: None,
            author_count: 1,
            loc,
            tag_counts: Default::default(),
            fan_in: 0,
            fan_out: 0,
            total_connections: 0,
            risk_score: 0.0,
            ..Default::default()
        }
    }

    fn chain(depth: u32) -> LodChain {
        LodChain {
            chain: "a.b.c".to_string(),
            depth,
            line: 1,
        }
    }

    fn cfg() -> ScoresConfig {
        ScoresConfig::default()
    }

    #[test]
    fn no_chains_score_100_no_violations() {
        let r = compute_lod(
            &[node("a.ts", 10), node("b.ts", 20)],
            &HashMap::new(),
            &cfg(),
        );
        assert_eq!(r.score, 100.0);
        assert_eq!(r.total_violations, 0);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn summarizes_a_files_chain_count_max_depth_and_density() {
        // a.ts: 3 chains depths [3,5,4] -> count 3, maxDepth 5, density 3/10 = 0.3
        // fileCount = 2 (both loc > 0); avgCount = 3 / 2 = 1.5
        // score = round((1 - 1.5/10) * 100) = round(85) = 85
        let lod_by_file: HashMap<String, Vec<LodChain>> =
            HashMap::from([("a.ts".to_string(), vec![chain(3), chain(5), chain(4)])]);
        let r = compute_lod(&[node("a.ts", 10), node("b.ts", 20)], &lod_by_file, &cfg());
        assert_eq!(r.score, 85.0);
        assert_eq!(r.total_violations, 3);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations[0].path, "a.ts");
        assert_eq!(r.violations[0].count, 3);
        assert_eq!(r.violations[0].max_depth, 5);
        assert_eq!(r.violations[0].loc, 10);
        assert_eq!(r.violations[0].density, 0.3);
    }

    #[test]
    fn avg_10_violations_per_file_score_0_boundary_and_stays_clamped_below_0() {
        // 1 file with loc > 0, 10 chains -> avgCount = 10/1 = 10 -> (1 - 1) * 100 = 0
        let at_10: HashMap<String, Vec<LodChain>> =
            HashMap::from([("a.ts".to_string(), (0..10).map(|_| chain(3)).collect())]);
        let r = compute_lod(&[node("a.ts", 5)], &at_10, &cfg());
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_violations, 10);

        // 20 chains -> avgCount = 20 -> (1 - 2) * 100 = -100 -> clamped to 0
        let above: HashMap<String, Vec<LodChain>> =
            HashMap::from([("a.ts".to_string(), (0..20).map(|_| chain(3)).collect())]);
        let r = compute_lod(&[node("a.ts", 5)], &above, &cfg());
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_violations, 20);
    }

    #[test]
    fn sorts_violations_by_density_descending() {
        // a.ts: count 2 / loc 100 = 0.02 ; b.ts: count 3 / loc 10 = 0.3
        let lod_by_file: HashMap<String, Vec<LodChain>> = HashMap::from([
            ("a.ts".to_string(), vec![chain(3), chain(3)]),
            ("b.ts".to_string(), vec![chain(3), chain(3), chain(3)]),
        ]);
        let r = compute_lod(&[node("a.ts", 100), node("b.ts", 10)], &lod_by_file, &cfg());
        let paths: Vec<&str> = r.violations.iter().map(|v| v.path.as_str()).collect();
        assert_eq!(paths, vec!["b.ts", "a.ts"]);
        assert!((r.violations[0].density - 0.3).abs() < 1e-10);
        assert!((r.violations[1].density - 0.02).abs() < 1e-10);
        // fileCount = 2, totalViolations = 5 -> avgCount = 2.5 -> round((1 - 0.25) * 100) = 75
        assert_eq!(r.score, 75.0);
    }

    #[test]
    fn defaults_loc_to_1_when_file_absent_from_nodes_and_ignores_loc_0_nodes_in_file_count() {
        // "ghost.ts" is in lodByFile but not in nodes -> loc falls back to 1, density = 2/1 = 2
        // nodes: a.ts loc 10 (counted), z.ts loc 0 (NOT counted) -> fileCount = 1
        // avgCount = totalViolations(2) / 1 = 2 -> round((1 - 0.2) * 100) = 80
        let lod_by_file: HashMap<String, Vec<LodChain>> =
            HashMap::from([("ghost.ts".to_string(), vec![chain(3), chain(3)])]);
        let r = compute_lod(&[node("a.ts", 10), node("z.ts", 0)], &lod_by_file, &cfg());
        assert_eq!(r.violations[0].path, "ghost.ts");
        assert_eq!(r.violations[0].count, 2);
        assert_eq!(r.violations[0].loc, 1);
        assert_eq!(r.violations[0].density, 2.0);
        assert_eq!(r.score, 80.0);
    }
}
