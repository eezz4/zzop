//! The aggregate [`Scores`] struct — the one type that names WHICH metrics exist, and therefore the
//! natural home for the contract binding every score to the population it scored over.
//!
//! Split out of the parent `reports.rs` on 2026-08-08 purely to stay under the per-file line cap, when
//! the population table pushed that file past it. The seam is the natural one: the parent keeps the
//! fifteen PER-METRIC report structs, this file keeps the aggregate and the cross-metric rules that only
//! make sense once every metric is in view.

use serde::{Deserialize, Serialize};

use super::*;

/// The aggregate score report — one field per structural-health metric.
///
/// # Every score ships its population (2026-08-08 user ruling)
///
/// The ruling, in the maintainer's framing: a measured score should not read as a bare 100 or 0 — it
/// should read as *problems found over subjects measured*. A score may never ship without the size of
/// the population it scored over, because 100 means two incompatible things and only the denominator
/// tells them apart: "this metric judged 4,000 subjects and every one passed" versus "this metric found
/// nothing it could judge". Both used to serialize as `score: 100.0`.
///
/// This is the same honesty policy `zzop_facade::query_coverage` states as its three-value cell rule,
/// which forbids folding measured and unmeasured axes into one number. The population is carried as a
/// FIELD ON THE SCORE rather than as a separate `unmeasured` array precisely because a denominator
/// cannot be dropped in transit the way a sibling list can — a consumer reading the score has the
/// population in its hand whether it wanted it or not.
///
/// **A population of 0 IS the "never measured" signal.** No separate boolean rides beside it: a flag
/// would be a second owner of the same fact, free to disagree with the number it describes.
///
/// The field carrying it differs per metric because the SUBJECT differs, and naming the subject is the
/// whole point (a bare `total` was measured to be the more misleading spelling — see `fixRatio`'s
/// 2026-07-31 renames). Ratio-shaped metrics ship the denominator of their own ratio; the two
/// DISTANCE-shaped metrics (`mainSequence`, `sdp`) are not ratios at all, so they ship the size of the
/// classified population instead:
///
/// | metric | population field | subject counted |
/// |---|---|---|
/// | `featureSlicedDesign` | `totalImports` | classified imports |
/// | `cohesion` | `sliceCount` | FSD slices found |
/// | `coupling` | `importerCount` | files with `fanOut > 0` |
/// | `sdp` | `totalCrossSliceEdges` | cross-slice edges |
/// | `hierarchy` | `totalIntraModuleEdges` | intra-module edges |
/// | `publicApi` | `totalCrossModuleImports` | cross-module imports |
/// | `fileSizeCompliance` | `total` | live source files |
/// | `mainSequence` | `classifiedFiles` | files with a KNOWN abstract/concrete kind |
/// | `modularity` | `edgeCount` | in-graph edges |
/// | `godFile` | `total` | live source files |
/// | `siblingCross` | `totalIntraModuleEdges` | intra-module edges |
/// | `diamond` | `rootsExamined` | scored roots walked |
/// | `renameInstability` | `total` | files in the git window |
/// | `busFactor` | `total` | live high-churn files |
/// | `fixRatio` | `taggedFileTouches` | tagged (file, commit) touches |
///
/// `zzop_metrics::health` reads exactly this column to decide which metrics may enter the composite —
/// see [`crate::health`] for the renormalization that depends on it.
///
/// # Two metrics were REMOVED rather than given a population (2026-08-08)
///
/// `typeSafety` and `lod` used to sit in this struct. Neither ever measured anything in any shipped
/// run: their sole production call site handed `compute_scores` an empty `type_safety_counts` and an
/// empty `lod_by_file`, and a whole-workspace producer census found not one non-test constructor of
/// `TypeSafetyCounts` or `LodChain` — no parser, rule or engine phase ever built either. So
/// `typeSafety` published `score: 100, totalAsCast: 0` on every TypeScript repo ever analyzed, and
/// `lod` published `score: 100` on everything.
///
/// Giving them a population would have been honest but pointless: the denominator would be a hardcoded
/// 0 forever, a permanent "never measured" cell for a capability nothing is building. Subtraction-first
/// (the repo's root principle) says remove them, and removing them takes their `SCORE_MEANINGS`
/// sentences with them, which is what stops the reply CLAIMING a measurement. Restoring either means
/// building its producer first — that is the honest order, and it was never the order these two had.
///
/// `mainSequence` was deliberately NOT removed with them, and it is not the same case: only its
/// abstractness input is dark. Its `instability` half is computed from the real dep graph on every run,
/// so deleting it would drop a live measurement no other metric publishes. It instead ships
/// `classifiedFiles`, which is 0 for as long as no classifier exists — the same blindness, now stated
/// by the metric itself rather than hidden behind a fabricated `abstractness: 0.0`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scores {
    pub feature_sliced_design: FeatureSlicedDesignScore,
    pub cohesion: CohesionScore,
    pub coupling: CouplingScore,
    pub sdp: SdpScore,
    pub hierarchy: HierarchyScore,
    pub public_api: PublicApiScore,
    pub file_size_compliance: FileSizeComplianceScore,
    pub main_sequence: MainSequenceScore,
    pub modularity: ModularityScore,
    pub god_file: GodFileScore,
    pub sibling_cross: SiblingCrossScore,
    pub diamond: DiamondScore,
    pub rename_instability: RenameScore,
    pub bus_factor: BusFactorScore,
    pub fix_ratio: FixRatioScore,
}
