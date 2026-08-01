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
pub struct FsdScore {
    pub score: f64,
    pub total_imports: u32,
    pub violations: Vec<FsdViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CohesionScore {
    pub score: f64,
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
pub struct SfcScore {
    pub score: f64,
    pub limit: u32,
    pub compliant: u32,
    pub total: u32,
    pub violations: Vec<SfcViolation>,
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

/// as-cast / any-type density — lower means higher TypeScript type confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeSafetyScore {
    pub score: f64,
    pub total_as_cast: u32,
    pub total_any_type: u32,
    pub violations: Vec<TypeSafetyViolation>,
    /// Rows the MAX_FILE_ROWS_LISTED cap dropped from `violations` (0 = complete). The two totals beside
    /// it are OCCURRENCE sums, not a row count, so neither one recovers this number.
    pub violations_truncated: u32,
}

/// Law of Demeter — a.b.c+ chain density. Lower means less indirect coupling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LodScore {
    pub score: f64,
    pub total_violations: u32,
    pub violations: Vec<LodFileSummary>,
    /// Rows the MAX_FILE_ROWS_LISTED cap dropped from `violations` (0 = complete). `total_violations`
    /// counts CHAINS across every file; this list has one row per FILE, so that field cannot stand in.
    pub violations_truncated: u32,
}

/// The aggregate score report — one field per structural-health metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scores {
    pub fsd: FsdScore,
    pub cohesion: CohesionScore,
    pub coupling: CouplingScore,
    pub sdp: SdpScore,
    pub hierarchy: HierarchyScore,
    pub public_api: PublicApiScore,
    pub sfc: SfcScore,
    pub main_sequence: MainSequenceScore,
    pub modularity: ModularityScore,
    pub god_file: GodFileScore,
    pub sibling_cross: SiblingCrossScore,
    pub diamond: DiamondScore,
    pub rename_instability: RenameScore,
    pub bus_factor: BusFactorScore,
    pub fix_ratio: FixRatioScore,
    pub type_safety: TypeSafetyScore,
    pub lod: LodScore,
}
