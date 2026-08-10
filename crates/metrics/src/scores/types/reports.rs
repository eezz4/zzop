//! Per-metric score report structs plus the aggregate `Scores` collection.

use serde::{Deserialize, Serialize};

use super::violations::*;

// ## `*Truncated` siblings (2026-07-31)
//
// Every capped detail list below carries a `<listField>_truncated` count of the rows its cap dropped —
// `0` when the list is complete. Before this, the caps were bare `Vec::truncate` calls and a full-looking
// list of 50 was indistinguishable from a truncated one, which is the exact silence the rest of this repo
// refuses (`findings` carries `{shown, truncated}`, `suggestionsTruncated`/`edgesTruncated` count what a
// cap left out, the graph lane prints a `%%` census). `list.len() + <listField>_truncated` is the honest
// full total, which most of these reports otherwise never publish.
//
// The cap VALUES and the one truncation helper live in `crate::scores::detail_cap`, not per module — see
// that module's doc for the two caps and the file-rows/edge-rows axis that separates them.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSlicedDesignScore {
    pub score: f64,
    /// The RATIO denominator — every in-tree import this metric looked at. Deliberately NOT the
    /// population: `score` is `violations / total_imports`, so this is the right divisor for the
    /// published number, but it counts imports whether or not FSD could classify either end.
    pub total_imports: u32,
    /// POPULATION: imports with at least one endpoint in a DECLARED FSD layer (entry / slice / shared).
    ///
    /// `total_imports` cannot answer "did this tree adopt the convention", and that is the question
    /// that decides whether the score means anything. An undeclared vocabulary puts every path in the
    /// catch-all layer, so no import can violate a layer rule and the metric returns a PERFECT 100 for
    /// a repo that has never heard of Feature-Sliced Design — measured on zzop's own tree, which scores
    /// 100 here on 2,795 imports, none of them FSD-classified. A `0` in this field is what tells those
    /// 2,795 apart from a real FSD repo whose layering is genuinely clean, and it is what keeps the
    /// metric's weight (2.5, the second highest) out of `health.pain` on trees it cannot judge.
    ///
    /// It is a SECOND count rather than a replacement divisor on purpose. Swapping the denominator
    /// would silently re-scale every existing score against the same configured thresholds, which this
    /// crate already refused once for the same reason (`CouplingScore::avg_fan_out_among_importers`:
    /// the population was stated in the name, and the computation was deliberately left alone).
    ///
    /// A LOW non-zero value is its own signal, and the sharpest one this field gives: it means the
    /// verdict rests on a handful of edges. Measured on a Go tree carrying the starter config, `api/`
    /// is the gRPC-Gateway generated-protobuf directory — the BOTTOM of that stack — while the starter
    /// `vocabulary.featureSlicedDesign.entry` lists `api` as a TOP entry-layer name, so 4 imports
    /// collided on the directory NAME and produced 4 layer-reverse violations that describe nothing
    /// about the code. Read against `layerClassifiedImports: 4` those violations are visibly resting on
    /// four edges; read against `totalImports: 10` they looked like a 40% failure rate. The fix for
    /// that tree is its own vocabulary (drop `api` from `entry`, or declare the block empty to switch
    /// the axis off) — the population makes the situation legible, it cannot guess the layout.
    pub layer_classified_imports: u32,
    pub violations: Vec<FeatureSlicedDesignViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CohesionScore {
    pub score: f64,
    /// POPULATION: FSD slices this metric found and scored. `0` means the tree exposed no slice the
    /// configured `vocabulary.featureSlicedDesign.sliceContainers` recognizes, so the score judged
    /// nothing — never that cohesion is perfect. Not recoverable from `slices.len()` as a contract: the
    /// list is a detail view and this is the denominator, and the two are allowed to diverge the moment
    /// a cap lands on the list (every other detail list here already has one).
    pub slice_count: u32,
    pub slices: Vec<SliceCohesion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouplingScore {
    pub score: f64,
    /// RENAMED from `avg_fan_out` (wire: `avgFanOut`) on 2026-07-31, user ruling. The old name read as
    /// "average fan-out across the files", and the denominator is not the files: a file with `fan_out ==
    /// 0` is dropped before the mean is taken, so every leaf, type-only module and entry-point-free file
    /// leaves the divisor. On a real tree that is a large share of the files, and the published number
    /// is materially HIGHER than the average a reader reproduced by hand — the arithmetic is not wrong,
    /// the population was unstated. The new name states the population instead: files that import
    /// something. `max_fan_out` needed no rename — a maximum over a subset that excludes zeroes is the
    /// same maximum. See [`crate::scores::coupling`] for why the exclusion itself was left alone.
    pub avg_fan_out_among_importers: f64,
    /// POPULATION: the count of files the average above is taken over — live, scored files with
    /// `fanOut > 0`. The 2026-07-31 rename put the population in the FIELD NAME
    /// (`avg_fan_out_among_importers`) but shipped no way to see how big it was, so a mean over 3
    /// importers and a mean over 3,000 read identically. `0` means no file in this tree imports
    /// anything in-tree — the score is 100 because there was nothing to average, which on a tree that
    /// obviously has imports is the import-RESOLUTION blindness signal (`coverage.declaredImportsByExt`
    /// high with the dep graph empty), not a clean bill of health.
    pub importer_count: u32,
    pub max_fan_out: f64,
    pub circular_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdpScore {
    pub score: f64,
    pub total_cross_slice_edges: u32,
    pub violations: Vec<SdpViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyScore {
    pub score: f64,
    pub total_intra_module_edges: u32,
    pub violations: Vec<HierarchyViolation>,
    /// Rows the MAX_EDGE_ROWS_LISTED cap dropped from `violations` (0 = complete). See the module header.
    pub violations_truncated: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicApiScore {
    pub score: f64,
    pub total_cross_module_imports: u32,
    pub deep_imports: Vec<DeepImport>,
    /// Rows the MAX_EDGE_ROWS_LISTED cap dropped from `deep_imports` (0 = complete). See the module header.
    pub deep_imports_truncated: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSizeComplianceScore {
    pub score: f64,
    pub limit: u32,
    pub compliant: u32,
    pub total: u32,
    pub violations: Vec<FileSizeComplianceViolation>,
    /// Rows the MAX_FILE_ROWS_LISTED cap dropped from `violations` (0 = complete). `total - compliant`
    /// already carries the UNCAPPED violation count (one violation per non-compliant live file), so the
    /// identity is `total - compliant == violations.len() + violations_truncated` — NOT
    /// `total - compliant == violations_truncated`. Both ship because a reader holding only this struct
    /// should not have to do that arithmetic to tell a full list from a cut one.
    pub violations_truncated: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainSequenceScore {
    pub score: f64,
    /// POPULATION (distance-shaped metric, so this is the CLASSIFIED population, not a ratio
    /// denominator): files whose abstract/concrete kind was actually known when `distance` was computed.
    ///
    /// **This is 0 on every shipped run**, and saying so is the entire reason the field exists. Martin's
    /// distance is `|abstractness + instability - 1|`; abstractness needs a per-file abstract/concrete
    /// classifier, and no producer of one exists anywhere in the workspace (the `FileKinds` input
    /// reaches `compute_scores` empty from its only production call site). Every module therefore got
    /// `abstractness: 0.0` — not measured-and-zero, but never measured — which silently turned the
    /// metric into `|instability - 1|` and made a module with no cross-module edges score distance 1.0
    /// (the worst possible) for the sole reason that nothing classified it.
    ///
    /// While this is 0, read `score`, `avgDistance`, and every row's `abstractness`/`distance` as
    /// UNMEASURED and read only `instability`/`fileCount`, which are computed from the real dep graph.
    /// `zzop_metrics::health` already does exactly that: a 0 here keeps `mainSequence` out of the
    /// composite entirely rather than letting a fabricated distance move `pain`.
    pub classified_files: u32,
    pub avg_distance: f64,
    pub modules: Vec<ModuleMainSeq>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModularityScore {
    pub score: f64,
    pub q: f64,
    pub edge_count: u32,
    pub slice_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GodFileScore {
    pub score: f64,
    pub limit: u32,
    /// POPULATION: live scored SOURCE files measured against `limit` — the denominator of the
    /// `gods/live` ratio behind `score`. `0` means no file reached this metric at all (no source
    /// dispatch class matched, or `exclude` removed them), so `score: 100` says nothing was weighed.
    pub total: u32,
    pub files: Vec<GodFile>,
    /// Rows the MAX_FILE_ROWS_LISTED cap dropped from `files` (0 = complete). This report publishes no
    /// other count of god files, so without it a 50-row list had no recoverable total at all.
    pub files_truncated: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiblingCrossScore {
    pub score: f64,
    pub total_intra_module_edges: u32,
    pub violations: Vec<SiblingCross>,
    /// Rows the MAX_EDGE_ROWS_LISTED cap dropped from `violations` (0 = complete). See the module header.
    pub violations_truncated: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiamondScore {
    pub score: f64,
    /// POPULATION: scored dep-graph roots this metric walked looking for two-hop diamonds. `0` means
    /// there was no graph to walk (no resolved import edges, or every root excluded), so the `100 -
    /// pairs x weight` formula started from a perfect score it never had a chance to lower.
    pub roots_examined: u32,
    pub pairs: Vec<DiamondPair>,
    /// Rows the MAX_FILE_ROWS_LISTED cap dropped from `pairs` (0 = complete). The score is computed from
    /// the FULL pair count before the cap, so without this the score and the list disagreed silently.
    pub pairs_truncated: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameScore {
    pub score: f64,
    pub renamed: u32,
    pub total: u32,
    pub files: Vec<RenamedFile>,
    /// Rows the MAX_FILE_ROWS_LISTED cap dropped from `files` (0 = complete) — `renamed` already carries
    /// the uncapped count, and the sibling still ships so the lane has ONE convention, not two.
    pub files_truncated: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusFactorScore {
    pub score: f64,
    /// POPULATION: live high-churn scored files (`loc > 0` and `changeCount >= minLiveChanges`) — the
    /// denominator of the `risky/total` ratio. It was already COMPUTED to produce `score` and simply
    /// never published, so a `risky: 0` could not be read against anything: 0-of-0 (the git window
    /// caught no file churning enough to judge) and 0-of-400 both shipped as `score: 100`.
    pub total: u32,
    pub risky: u32,
    pub files: Vec<BusFactorFile>,
    /// Rows the MAX_FILE_ROWS_LISTED cap dropped from `files` (0 = complete) — `risky` already carries the
    /// uncapped count, and the sibling still ships so the lane has ONE convention, not two.
    pub files_truncated: u32,
}

/// FIX-work share — lower means fewer reactive fixes. Score 0 at 30%, 100 at 0%. The three counters
/// below are FILE TOUCHES, not commits; see [`crate::scores::fix_ratio`] for why the distinction is the
/// whole point of their names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixRatioScore {
    pub score: f64,
    /// RENAMED from `fix` (wire: `fix`) on 2026-07-31, user ruling. The old name sat next to a module
    /// doc reading "fix commits", so it read as a COMMIT count. It is not one: a commit's type tag is
    /// recorded once per FILE the commit touched, so a single `[FIX]` commit spanning three scored files
    /// adds three. The new name states the membership rule — one (file, tagged commit) touch.
    pub fix_file_touches: u32,
    /// RENAMED from `total` (wire: `total`) on 2026-07-31, user ruling. `total` is the worst of the
    /// three: it was neither total commits nor total file touches. It counts only touches whose commit
    /// matched SOME commit-type pattern — a commit matching none contributes 0 and is invisible to both
    /// sides of the ratio. The new name states that: TAGGED file touches.
    pub tagged_file_touches: u32,
    /// RENAMED from `ratio` (wire: `ratio`) on 2026-07-31, user ruling. `fix_file_touches /
    /// tagged_file_touches` — a share of the TAGGED touches, not of all commits, so the untagged
    /// remainder never enters the denominator. The name now carries the denominator, because that is
    /// the term a reader was silently substituting.
    pub fix_share_of_tagged_touches: f64,
}

mod aggregate;

pub use aggregate::Scores;
