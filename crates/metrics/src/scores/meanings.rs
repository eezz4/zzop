//! What each structural score MEANS, in one sentence — shipped in the reply itself as `scoreMeanings`.
//!
//! # Why this exists
//!
//! Fifteen score keys ride every git-enabled reply, and four of them used to be bare acronyms: `sdp`,
//! `sfc`, `lod`, `fsd`. A 2026-08-04 name survey measured where a reader could learn what they stand for
//! and found the answer only in Rust doc-comments — `grep -ln "Stable Dep"` hit exactly one file, and
//! `docs/`/`site/` had zero. A consumer holding `scores.sdp = 41.2` had no vocabulary anywhere, and an
//! agent had less than that: the MCP tool descriptions do not name these fields at all.
//!
//! Two of the four were later renamed on the wire rather than merely explained here, because a legend
//! cannot fix a name that points at the wrong thing. `sfc` -> `fileSizeCompliance`: the score is a
//! LOC-cap compliance ratio, but its letters read as Vue's Single-File Component, which this same
//! product uses `SFC` for in `docs/rules/catalog.md`, `docs/NORMALIZED_AST.md` and the published
//! `envelope.schema.json` — one spelling, two meanings, both user-facing. `fsd` ->
//! `featureSlicedDesign`: correct letters, but four characters a reader had no way to expand.
//!
//! `sdp` stays abbreviated on purpose: it abbreviates a PROPER NAME (Stable Dependencies Principle), so
//! spelling the key out would not tell a reader anything the sentence below does not already tell them
//! better. That is why this table did not go away with the renames. (`lod` — Law of Demeter — was the
//! other such key until 2026-08-08, when the score itself was removed for never having measured
//! anything; see [`crate::scores::types::Scores`].)
//!
//! # Why a shipped field rather than a documentation table
//!
//! This repo already solved this shape once: `zzop_facade::query`'s `verdict_meaning` puts the eight
//! endpoint-verdict tokens into the reply as `verdictMeaning`, because explaining them in the MCP tool
//! description alone left the CLI user with a bare token. Its doc says the rest — copying a table into a
//! second surface gives one fact two owners, which is the drift class this repo keeps paying for. A
//! documentation table would have exactly that defect: nothing would make it move when a score does.
//!
//! So the definitions live here, once, and reach every surface by riding the reply.
//!
//! # What holds this complete
//!
//! [`tests::every_score_field_has_a_meaning`] serializes `Scores` and compares the wire key set against
//! [`SCORE_MEANINGS`]'s keys, in BOTH directions. The subject set is therefore serde's own output, not a
//! list written here — an eighteenth score turns that test red without anyone remembering to.

/// Every score key with its one-sentence definition, in `Scores` declaration order.
///
/// The key is the JSON field name (serde camelCase), so this table is addressed exactly the way a
/// consumer addresses the score it explains. Each sentence answers "what does a LOW number mean here",
/// because every score is 0–100 with higher being healthier and that direction is the first thing a
/// reader gets wrong.
pub const SCORE_MEANINGS: &[(&str, &str)] = &[
    (
        "featureSlicedDesign",
        "Feature-Sliced Design layering: the share of imports that do NOT reverse a layer or cross \
         between slices of the same layer. Low means modules import upward or reach sideways into each \
         other's internals. The layer names are yours to declare (`vocabulary.featureSlicedDesign`).",
    ),
    (
        "cohesion",
        "Slice cohesion: how much of a slice's import traffic stays inside itself. Low means slices are \
         mostly wiring to other slices rather than doing their own work.",
    ),
    (
        "coupling",
        "Average fan-out among files that import anything, plus a circular-reference penalty. Low means \
         the typical file depends on many others, so a change has a wide blast radius.",
    ),
    (
        "sdp",
        "Stable Dependencies Principle (Robert Martin): the share of cross-module imports that flow \
         TOWARD more stable modules. Low means volatile modules are being depended on, so their churn \
         propagates outward.",
    ),
    (
        "hierarchy",
        "Directory hierarchy respect: the share of intra-module imports that do NOT point from a child \
         directory up to an ancestor. Low means the folder tree does not describe the dependency \
         direction.",
    ),
    (
        "publicApi",
        "Barrel discipline: the share of cross-module imports that land on another module's TOP LEVEL \
         — its `index` barrel or any other file directly under it — rather than deep-pathing into a \
         subdirectory. Low means module boundaries are routinely bypassed. This one is a convention, \
         not a defect: a project that deliberately deep-imports is not wrong, it scores low. READ IT \
         AGAINST `totalCrossModuleImports`, because a MODULE here is a declared feature-sliced slice \
         or base directory, else the first path segment — so a repository that keeps all its code \
         under one top-level directory (a single `src/`, a Maven `src/main/`) has almost no imports \
         that cross a module at all, and scores 100 on an EMPTY denominator. A 100 beside \
         `totalCrossModuleImports: 0` means this tree's layout gave the metric nothing to judge, \
         never that its boundaries are clean; `health.contributors` says the same thing there by \
         reporting `population: 0` and dropping the metric from `pain`.",
    ),
    (
        "fileSizeCompliance",
        "One-file-one-responsibility: the share of files within the per-target line limit. Low means \
         many oversized files.",
    ),
    (
        "mainSequence",
        "Main Sequence distance (Robert Martin): how close each module sits to the ideal balance of \
         abstractness and instability. Low means modules are either rigid-and-concrete or \
         abstract-and-unused.",
    ),
    (
        "modularity",
        "Newman Q modularity: how cleanly the import graph splits into the declared modules. Low means \
         the module boundaries do not match how the code actually clusters.",
    ),
    (
        "godFile",
        "Absence of god files — files past TWICE the `file_size_compliance` line limit. Low means a few files carry a \
         disproportionate share of the codebase.",
    ),
    (
        "siblingCross",
        "Sibling isolation: the share of intra-module imports that do NOT run horizontally between \
         sibling sub-directories. Low means peers reach into each other instead of through a shared \
         parent.",
    ),
    (
        "diamond",
        "Absence of diamond dependencies — two paths from one root to one leaf. Low means many such \
         shapes, which make a change's reach hard to predict. A diamond is not automatically a defect.",
    ),
    (
        "renameInstability",
        "Naming stability: the share of files never renamed in the analyzed git window. Low means \
         churn in what things are CALLED, which breaks every reader's mental index.",
    ),
    (
        "busFactor",
        "Knowledge spread: the share of high-churn files with more than one author in the analyzed \
         window. Low means the code that changes most is understood by one person.",
    ),
    (
        "fixRatio",
        "Share of tagged history that is NOT reactive fixing. Low means much of the recorded work is \
         repair rather than change. Needs a recognized commit-message convention (`git.commitTypePatterns`).",
    ),
];

/// The definition for one score key, or `None` for a key this table does not know.
///
/// `None` cannot occur for a key taken from a serialized `Scores` (the completeness test forbids it);
/// it exists because a caller may pass an arbitrary string and inventing a sentence would be worse than
/// saying nothing.
pub fn score_meaning(key: &str) -> Option<&'static str> {
    SCORE_MEANINGS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, m)| *m)
}

#[cfg(test)]
mod tests;
