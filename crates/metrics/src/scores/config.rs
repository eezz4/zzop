//! Explicit scores configuration — a struct threaded explicitly through call sites instead of relying on
//! module-level mutable global state, so no one-time setup call is needed before use and multiple configurations
//! can coexist.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
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

/// typeSafety: as-cast/any density at which the score reaches 0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TypeSafetyThresholds {
    pub density_cap: f64,
}

impl Default for TypeSafetyThresholds {
    fn default() -> Self {
        TypeSafetyThresholds { density_cap: 0.1 }
    }
}

/// lod: avg Law-of-Demeter violations/file at which the score reaches 0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LodThresholds {
    pub count_cap: f64,
}

impl Default for LodThresholds {
    fn default() -> Self {
        LodThresholds { count_cap: 10.0 }
    }
}

/// godFile: LOC threshold = SFC limit x locMultiplier; score penalty = (gods/live) x penaltySlope.
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
    pub type_safety: TypeSafetyThresholds,
    pub lod: LodThresholds,
    pub god_file: GodFileThresholds,
    /// sfc / godFile: per-role Single-File-Component LOC limit table. Open-keyed (roles are config-derived) —
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
            type_safety: TypeSafetyThresholds::default(),
            lod: LodThresholds::default(),
            god_file: GodFileThresholds::default(),
            loc_limits,
            coupling: CouplingThresholds::default(),
            modularity: ModularityThresholds::default(),
            diamond: DiamondThresholds::default(),
        }
    }
}

/// Feature-Sliced Design vocabulary — the per-repo directory conventions that drive `classify_path`/`module_of`.
/// A generic FSD repo needs no overrides; the derived `Default` impl's values apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsdConfig {
    /// L2 slice containers — first subdirectory is the slice (e.g. features/auth).
    pub slice_containers: Vec<String>,
    /// L1 entry layer prefixes.
    pub entry: Vec<String>,
    /// L3 shared layer prefixes.
    pub shared: Vec<String>,
    /// Foundation directory names — paths containing `/{dir}/` are classified as base modules (L4).
    pub base_dirs: Vec<String>,
}

impl Default for FsdConfig {
    fn default() -> Self {
        FsdConfig {
            slice_containers: vec!["features".to_string(), "domains".to_string()],
            entry: vec!["pages".to_string(), "routes".to_string(), "api".to_string()],
            shared: vec![
                "core".to_string(),
                "hooks".to_string(),
                "render".to_string(),
                "ui".to_string(),
                "shared".to_string(),
                "lib".to_string(),
                "utils".to_string(),
                "__test__".to_string(),
            ],
            base_dirs: vec!["base".to_string()],
        }
    }
}

/// Precompiled FSD regexes bundled with the config that produced them — an explicitly constructed value passed
/// at call sites rather than module-level mutable globals. Not `Serialize`/`Deserialize`: it holds compiled
/// `Regex` values, so it is treated as engine config, not analysis output (unlike `ScoreThresholds`).
#[derive(Debug, Clone)]
pub struct FsdMatcher {
    pub config: FsdConfig,
    pub entry_re: Regex,
    pub slice_re: Regex,
    pub shared_re: Regex,
    pub base_re: Regex,
}

impl FsdMatcher {
    /// Precompiles the four FSD regexes from `config`.
    pub fn new(config: FsdConfig) -> Self {
        fn alt(xs: &[String]) -> String {
            xs.join("|")
        }
        let entry_re =
            Regex::new(&format!("^({})/", alt(&config.entry))).expect("valid entry regex");
        let slice_re = Regex::new(&format!("^({})/([^/]+)/", alt(&config.slice_containers)))
            .expect("valid slice regex");
        let shared_re =
            Regex::new(&format!("^({})/", alt(&config.shared))).expect("valid shared regex");
        let base_re = Regex::new(&format!("/({})/([^/]+)/", alt(&config.base_dirs)))
            .expect("valid base regex");
        FsdMatcher {
            config,
            entry_re,
            slice_re,
            shared_re,
            base_re,
        }
    }
}

impl Default for FsdMatcher {
    fn default() -> Self {
        FsdMatcher::new(FsdConfig::default())
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
    pub fsd: FsdMatcher,
}

impl Default for ScoresConfig {
    fn default() -> Self {
        ScoresConfig {
            thresholds: ScoreThresholds::default(),
            hierarchy_shared_dirs: [
                "utils",
                "types",
                "helpers",
                "hooks",
                "constants",
                "lib",
                "display",
                "__test__",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            fsd: FsdMatcher::default(),
        }
    }
}
