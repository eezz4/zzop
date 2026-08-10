//! Explicit scores configuration — a struct threaded explicitly through call sites instead of relying on
//! module-level mutable global state, so no one-time setup call is needed before use and multiple configurations
//! can coexist.

use std::collections::{BTreeMap, BTreeSet};

mod feature_sliced_design;

pub use feature_sliced_design::{
    FeatureSlicedDesignConfig, FeatureSlicedDesignMatcher, DEFAULT_FSD_BASE_DIRS,
    DEFAULT_FSD_ENTRY, DEFAULT_FSD_SHARED, DEFAULT_FSD_SLICE_CONTAINERS,
};
use serde::{Deserialize, Serialize};

/// Fallback Single-File-Component LOC limit for a role with no `loc_limits` entry.
/// A pure normalization constant — not user-tunable.
pub const DEFAULT_LOC_LIMIT: u32 = 150;

/// busFactor: min changeCount for a file to count as "live" (knowledge-isolation gate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusFactorThresholds {
    pub min_live_changes: u32,
}

impl Default for BusFactorThresholds {
    fn default() -> Self {
        BusFactorThresholds {
            min_live_changes: 10,
        }
    }
}

/// fixRatio: FIX/total ratio at which the score reaches 0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FixRatioThresholds {
    pub cap: f64,
}

impl Default for FixRatioThresholds {
    fn default() -> Self {
        FixRatioThresholds { cap: 0.3 }
    }
}

/// godFile: LOC threshold = the file-size-compliance LOC limit x locMultiplier; score penalty = (gods/live) x penaltySlope.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GodFileThresholds {
    pub loc_multiplier: f64,
    pub penalty_slope: f64,
}

impl Default for GodFileThresholds {
    fn default() -> Self {
        GodFileThresholds {
            loc_multiplier: 2.0,
            penalty_slope: 200.0,
        }
    }
}

/// coupling: fan-out penalty starts above `fan_out_knee` avg, at `fan_out_slope`/unit; circular penalty =
/// min(circular_cap, count x circular_weight).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CouplingThresholds {
    pub fan_out_knee: f64,
    pub fan_out_slope: f64,
    pub circular_cap: f64,
    pub circular_weight: f64,
}

impl Default for CouplingThresholds {
    fn default() -> Self {
        CouplingThresholds {
            fan_out_knee: 5.0,
            fan_out_slope: 10.0,
            circular_cap: 30.0,
            circular_weight: 5.0,
        }
    }
}

/// modularity: Newman-Q value treated as "good" (score reaches 100); score = (q / target_q) x 100.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModularityThresholds {
    pub target_q: f64,
}

impl Default for ModularityThresholds {
    fn default() -> Self {
        ModularityThresholds { target_q: 0.3 }
    }
}

/// diamond: score penalty per diamond pair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DiamondThresholds {
    pub penalty_weight: f64,
}

impl Default for DiamondThresholds {
    fn default() -> Self {
        DiamondThresholds {
            penalty_weight: 2.0,
        }
    }
}

/// Health-score policy thresholds — tunable knobs of the per-metric score formulas in scores/*.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreThresholds {
    pub bus_factor: BusFactorThresholds,
    pub fix_ratio: FixRatioThresholds,
    pub god_file: GodFileThresholds,
    /// file_size_compliance / godFile: per-role LOC limit table (the cap `fileSizeCompliance` scores against — NOT Vue's Single-File Component). Open-keyed (roles are config-derived) —
    /// `fe`/`be`/`all` are the universal defaults; a role with no entry falls back to `DEFAULT_LOC_LIMIT`.
    pub loc_limits: BTreeMap<String, u32>,
    pub coupling: CouplingThresholds,
    pub modularity: ModularityThresholds,
    pub diamond: DiamondThresholds,
}

impl ScoreThresholds {
    /// Looks up the LOC limit for a role, falling back to `DEFAULT_LOC_LIMIT` for an unknown/absent role.
    /// Never panics on an unrecognized role — the role vocabulary is open (a config may declare any string,
    /// e.g. "worker", "mobile").
    pub fn loc_limit(&self, role: Option<&str>) -> u32 {
        role.and_then(|r| self.loc_limits.get(r))
            .copied()
            .unwrap_or(DEFAULT_LOC_LIMIT)
    }
}

impl Default for ScoreThresholds {
    fn default() -> Self {
        let mut loc_limits = BTreeMap::new();
        loc_limits.insert("fe".to_string(), 100);
        loc_limits.insert("be".to_string(), 200);
        loc_limits.insert("all".to_string(), 200);
        ScoreThresholds {
            bus_factor: BusFactorThresholds::default(),
            fix_ratio: FixRatioThresholds::default(),
            god_file: GodFileThresholds::default(),
            loc_limits,
            coupling: CouplingThresholds::default(),
            modularity: ModularityThresholds::default(),
            diamond: DiamondThresholds::default(),
        }
    }
}

/// The scores subsystem's full configuration — bundles the threshold knobs, the shared/cross-cutting dir
/// vocabulary, and the FSD matcher that every scores/* module needs. Threaded explicitly through call sites
/// (see module doc comment for why this uses an explicit struct instead of global state).
#[derive(Debug, Clone)]
pub struct ScoresConfig {
    /// Per-metric formula thresholds.
    pub thresholds: ScoreThresholds,
    /// scores/hierarchy · scores/siblingCross — path-segment vocabulary for shared/cross-cutting dirs (utils,
    /// types, hooks, ...). A sub-directory in this set is exempt from upward-import / sibling-cross violations
    /// (it is shared infra, not a layer).
    pub hierarchy_shared_dirs: BTreeSet<String>,
    /// FSD directory-convention matcher, held here instead of as global state.
    pub feature_sliced_design: FeatureSlicedDesignMatcher,
}

/// Cross-cutting directory names exempt from layering violations when a project declares none — the
/// value `vocabulary.hierarchySharedDirs` replaces. Deliberately a SEPARATE axis from
/// [`DEFAULT_FSD_SHARED`] despite the overlapping words: this one decides what is exempt from
/// upward-import / sibling-cross checks, that one names an FSD layer. Flattening them into one list
/// would give two questions one answer.
pub const DEFAULT_HIERARCHY_SHARED_DIRS: &[&str] = &[
    "utils",
    "types",
    "helpers",
    "hooks",
    "constants",
    "lib",
    "display",
    "__test__",
];

impl Default for ScoresConfig {
    fn default() -> Self {
        ScoresConfig {
            thresholds: ScoreThresholds::default(),
            hierarchy_shared_dirs: DEFAULT_HIERARCHY_SHARED_DIRS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            feature_sliced_design: FeatureSlicedDesignMatcher::default(),
        }
    }
}
