//! as-cast / any-type density score — lower density means higher TypeScript type confidence.
//! Scale: 10% density -> 0, 0% -> 100.

use std::collections::HashMap;

use crate::scores::config::ScoresConfig;
use crate::scores::types::{TypeSafetyScore, TypeSafetyViolation};
use zzop_core::FileNode;

/// Max detail rows returned.
const MAX_DETAIL_ITEMS: usize = 50;
/// The 0-100 score scale.
const PERCENT: f64 = 100.0;

/// Per-file `as`-cast and `any`-typed occurrence counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TypeSafetyCounts {
    pub as_cast: u32,
    pub any_type: u32,
}

/// `avg_density = (total_as_cast + total_any_type) / total_loc` over files present in `counts`;
/// `score = max(0, round((1 - avg_density / density_cap) * 100))`. A path in `counts` but absent from `nodes`
/// falls back to `loc = 1` (a single occurrence there reads as maximal density rather than being silently diluted).
pub fn compute_type_safety(
    nodes: &[FileNode],
    counts: &HashMap<String, TypeSafetyCounts>,
    cfg: &ScoresConfig,
) -> TypeSafetyScore {
    let loc_by_path: HashMap<&str, u32> = nodes.iter().map(|n| (n.path.as_str(), n.loc)).collect();

    let mut total_as_cast: u32 = 0;
    let mut total_any_type: u32 = 0;
    let mut total_loc: u64 = 0;
    let mut violations: Vec<TypeSafetyViolation> = Vec::new();

    for (path, c) in counts {
        let loc = loc_by_path.get(path.as_str()).copied().unwrap_or(1);
        total_as_cast += c.as_cast;
        total_any_type += c.any_type;
        total_loc += u64::from(loc);
        violations.push(TypeSafetyViolation {
            path: path.clone(),
            as_cast: c.as_cast,
            any_type: c.any_type,
            loc,
            density: f64::from(c.as_cast + c.any_type) / f64::from(loc.max(1)),
        });
    }

    // Deterministic tie-break by path — the source is a HashMap, which has no defined iteration order.
    violations.sort_by(|a, b| {
        b.density
            .partial_cmp(&a.density)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    let avg_density = if total_loc > 0 {
        f64::from(total_as_cast + total_any_type) / total_loc as f64
    } else {
        0.0
    };
    let density_cap = cfg.thresholds.type_safety.density_cap;
    let score = ((1.0 - avg_density / density_cap) * PERCENT)
        .round()
        .max(0.0);

    violations.truncate(MAX_DETAIL_ITEMS);

    TypeSafetyScore {
        score,
        total_as_cast,
        total_any_type,
        violations,
    }
}

#[cfg(test)]
mod tests {
    //! Covers the empty-counts baseline, zero casts over real LOC, the score floor at and above the
    //! density cap, a mid-range density with violations sorted by density descending, and the loc=1
    //! fallback for a path present in `counts` but absent from `nodes`.
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

    fn cfg() -> ScoresConfig {
        ScoresConfig::default()
    }

    fn counts(pairs: &[(&str, u32, u32)]) -> HashMap<String, TypeSafetyCounts> {
        pairs
            .iter()
            .map(|(p, a, t)| {
                (
                    p.to_string(),
                    TypeSafetyCounts {
                        as_cast: *a,
                        any_type: *t,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn empty_counts_total_loc_0_avg_density_0_score_100() {
        let r = compute_type_safety(&[node("a", 100)], &HashMap::new(), &cfg());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.total_as_cast, 0);
        assert_eq!(r.total_any_type, 0);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn zero_casts_over_real_loc_density_0_score_100() {
        let r = compute_type_safety(&[node("a", 100)], &counts(&[("a", 0, 0)]), &cfg());
        assert_eq!(r.score, 100.0);
        assert_eq!(
            r.violations,
            vec![TypeSafetyViolation {
                path: "a".to_string(),
                as_cast: 0,
                any_type: 0,
                loc: 100,
                density: 0.0,
            }]
        );
    }

    #[test]
    fn density_at_the_10_percent_cap_score_0() {
        // asCast+anyType = 10 over loc 100 -> avgDensity 0.1 -> (1 - 0.1/0.1)*100 = 0
        let r = compute_type_safety(&[node("a", 100)], &counts(&[("a", 6, 4)]), &cfg());
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_as_cast, 6);
        assert_eq!(r.total_any_type, 4);
    }

    #[test]
    fn density_above_cap_score_floors_at_0() {
        // 20 casts over loc 100 -> density 0.2 -> (1 - 2)*100 = -100 -> max(0,..) = 0
        let r = compute_type_safety(&[node("a", 100)], &counts(&[("a", 20, 0)]), &cfg());
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn density_5_percent_score_50_violations_sorted_by_density_desc() {
        // totals: asCast+anyType = 5, totalLoc = 100 -> avgDensity 0.05 -> (1 - 0.5)*100 = 50
        // file "a": 4/40 = 0.1 density; file "b": 1/60 ~ 0.0167 density -> a sorts first
        let r = compute_type_safety(
            &[node("a", 40), node("b", 60)],
            &counts(&[("a", 2, 2), ("b", 1, 0)]),
            &cfg(),
        );
        assert_eq!(r.score, 50.0);
        assert_eq!(r.total_as_cast, 3);
        assert_eq!(r.total_any_type, 2);
        let paths: Vec<&str> = r.violations.iter().map(|v| v.path.as_str()).collect();
        assert_eq!(paths, vec!["a", "b"]);
    }

    #[test]
    fn missing_node_loc_falls_back_to_1() {
        // path "ghost" not in nodes -> loc defaults to 1; 1 cast over loc 1 -> density 1.0
        // avgDensity = 1/1 = 1 -> (1 - 10)*100 -> 0
        let r = compute_type_safety(&[], &counts(&[("ghost", 1, 0)]), &cfg());
        assert_eq!(r.violations[0].loc, 1);
        assert_eq!(r.violations[0].density, 1.0);
        assert_eq!(r.score, 0.0);
    }
}
