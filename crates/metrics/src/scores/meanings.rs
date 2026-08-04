//! What each structural score MEANS, in one sentence — shipped in the reply itself as `scoreMeanings`.
//!
//! # Why this exists
//!
//! Seventeen score keys ride every git-enabled reply, and four of them are bare acronyms: `sdp`, `sfc`,
//! `lod`, `fsd`. A 2026-08-04 name survey measured where a reader could learn what they stand for and
//! found the answer only in Rust doc-comments — `grep -ln "Stable Dep"` hit exactly one file, and
//! `docs/`/`site/` had zero. A consumer holding `scores.sdp = 41.2` had no vocabulary anywhere, and an
//! agent had less than that: the MCP tool descriptions do not name these fields at all.
//!
//! `sfc` was worse than opaque. The score means "Single File Component" as in one-file-one-responsibility
//! (LOC-cap compliance), while the same product's docs use `SFC` for Vue's Single-File Component — in
//! `docs/rules/catalog.md`, `docs/NORMALIZED_AST.md`, and the published `envelope.schema.json`. One
//! spelling, two meanings, both user-facing.
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
        "fsd",
        "Feature-Sliced Design layering: the share of imports that do NOT reverse a layer or cross \
         between slices of the same layer. Low means modules import upward or reach sideways into each \
         other's internals. The layer names are yours to declare (`vocabulary.fsd`).",
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
        "Barrel discipline: the share of cross-module imports that go through a module's index rather \
         than deep-pathing into its internals. Low means module boundaries are routinely bypassed.",
    ),
    (
        "sfc",
        "One-file-one-responsibility: the share of files within the per-target line limit. Low means \
         many oversized files. NOTE: this is NOT Vue's Single-File Component — the two are unrelated \
         despite the shared initials.",
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
        "Absence of god files — files past TWICE the `sfc` line limit. Low means a few files carry a \
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
    (
        "typeSafety",
        "TypeScript confidence: inverse density of `as` casts and `any` types. Low means the type system \
         is being told to stand down often.",
    ),
    (
        "lod",
        "Law of Demeter: inverse density of property-access chains (`a.b.c` and longer). Low means callers \
         reach through their neighbours' internals, so a change deep in a chain surfaces far away. NOT \
         Level of Detail.",
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
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The score keys as they actually reach the wire — read off a REAL `Scores`, produced by the same
    /// `compute_scores` every run calls, then serialized by the same serde derive the reply uses. An
    /// empty graph is enough: the field set does not depend on the input, and going through the real
    /// constructor is what keeps this pin honest (a hand-built literal here would be a second
    /// definition of "which scores exist", which is the defect this file exists to prevent).
    fn wire_keys() -> BTreeSet<String> {
        use crate::scores::compute::{compute_scores, ScoresInput};
        use crate::scores::types::FileKinds;
        use crate::scores::ScoresConfig;
        use std::collections::HashMap;

        let dep = Default::default();
        let file_kinds = FileKinds::default();
        let type_safety_counts = HashMap::new();
        let lod_by_file = HashMap::new();
        let yes: &dyn Fn(&str) -> bool = &|_| true;
        let scores = compute_scores(
            &ScoresInput {
                nodes: &[],
                dep: &dep,
                circular: &[],
                target: None,
                file_kinds: &file_kinds,
                type_safety_counts: &type_safety_counts,
                lod_by_file: &lod_by_file,
                is_source: yes,
                is_scored: yes,
            },
            &ScoresConfig::default(),
        );
        let value = serde_json::to_value(scores).expect("Scores serializes");
        value
            .as_object()
            .expect("Scores is a JSON object")
            .keys()
            .cloned()
            .collect()
    }

    /// The completeness pin, in both directions, against serde's own output rather than a list written
    /// by hand: a new score field turns this red, and a meaning for a field that no longer exists does
    /// too. Working-agreements §5.5① — the subject set is derived, never authored.
    #[test]
    fn every_score_field_has_a_meaning() {
        let wire = wire_keys();
        let documented: BTreeSet<String> =
            SCORE_MEANINGS.iter().map(|(k, _)| k.to_string()).collect();
        assert!(
            wire.len() >= 10,
            "extraction floor: serializing Scores yielded {} keys, which means the derivation broke and \
             this pin would pass vacuously",
            wire.len()
        );
        let missing: Vec<_> = wire.difference(&documented).collect();
        assert!(
            missing.is_empty(),
            "these score fields ship on the wire with no meaning beside them: {missing:?} — add a row \
             to SCORE_MEANINGS. A bare key is exactly what this table exists to stop."
        );
        let stale: Vec<_> = documented.difference(&wire).collect();
        assert!(
            stale.is_empty(),
            "SCORE_MEANINGS explains keys that no score emits: {stale:?} — the exemption side of the \
             same check (working-agreements §5.5②), so a removed score cannot leave its sentence behind."
        );
    }

    /// The two collision-carrying entries must keep saying what they are NOT. Both initials are live in
    /// this same product with a different meaning — `SFC` as Vue's Single-File Component in
    /// `docs/NORMALIZED_AST.md` and the published envelope schema, `LOD` as Level of Detail in the
    /// graphics sense a reader may well arrive with.
    #[test]
    fn the_two_colliding_acronyms_disclaim_the_other_reading() {
        assert!(
            score_meaning("sfc")
                .expect("sfc has a meaning")
                .contains("NOT Vue"),
            "sfc's sentence must disown Vue's Single-File Component"
        );
        assert!(
            score_meaning("lod")
                .expect("lod has a meaning")
                .contains("NOT Level of Detail"),
            "lod's sentence must disown Level of Detail"
        );
    }
}
