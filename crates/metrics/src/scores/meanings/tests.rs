//! Pins for [`super::SCORE_MEANINGS`] and for the population contract every score must satisfy.
//!
//! Split out of the parent on 2026-08-08 to stay under the per-file line cap when the
//! `every_score_ships_the_population_it_scored_over` pin landed. Both pins here derive their subject
//! set from serde output rather than from a hand-written list, which is what makes them survive a new
//! score being added by someone who never reads this file.

use super::*;
use std::collections::BTreeSet;

/// The score keys as they actually reach the wire — read off a REAL `Scores`, produced by the same
/// `compute_scores` every run calls, then serialized by the same serde derive the reply uses. An
/// empty graph is enough: the field set does not depend on the input, and going through the real
/// constructor is what keeps this pin honest (a hand-built literal here would be a second
/// definition of "which scores exist", which is the defect this file exists to prevent).
fn wire_scores() -> serde_json::Value {
    use crate::scores::compute::{compute_scores, ScoresInput};
    use crate::scores::types::FileKinds;
    use crate::scores::ScoresConfig;

    let dep = Default::default();
    let file_kinds = FileKinds::default();
    let yes: &dyn Fn(&str) -> bool = &|_| true;
    let scores = compute_scores(
        &ScoresInput {
            nodes: &[],
            dep: &dep,
            circular: &[],
            target: None,
            file_kinds: &file_kinds,
            is_source: yes,
            is_scored: yes,
        },
        &ScoresConfig::default(),
    );
    serde_json::to_value(scores).expect("Scores serializes")
}

fn wire_keys() -> BTreeSet<String> {
    wire_scores()
        .as_object()
        .expect("Scores is a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// RED (defects b and c): **no score may ship without its population.**
///
/// Same derivation discipline as the meanings pin below — the subject set is serde's own output, so
/// a sixteenth score cannot land without answering "over what?". The population field per metric is
/// declared once, in [`crate::scores::types::Scores`]'s table, and this crosses that declaration
/// against the real wire shape in BOTH directions: a score with no declared population fails, and a
/// declared population naming a key the score does not actually serialize fails too.
///
/// This is the pin that would have caught all three faces of the 2026-08-08 defect at once.
/// `typeSafety` shipped `score: 100` with a `totalAsCast: 0` that was hardcoded-empty rather than
/// measured; `lod` shipped `score: 100` with no producer in the workspace; `featureSlicedDesign`
/// shipped `score: 0` on a Go tree whose `api/` directory collided with an FSD entry-layer name. In
/// every case the population was the missing sentence, and in every case nothing failed.
#[test]
fn every_score_ships_the_population_it_scored_over() {
    /// The population field for each score key — the wire spelling of
    /// `Scores`' population table. Ratio-shaped metrics name their own denominator; the two
    /// distance-shaped ones (`mainSequence`, `sdp`) name their classified population instead.
    const POPULATION_FIELD: &[(&str, &str)] = &[
        ("featureSlicedDesign", "layerClassifiedImports"),
        ("cohesion", "sliceCount"),
        ("coupling", "importerCount"),
        ("sdp", "totalCrossSliceEdges"),
        ("hierarchy", "totalIntraModuleEdges"),
        ("publicApi", "totalCrossModuleImports"),
        ("fileSizeCompliance", "total"),
        ("mainSequence", "classifiedFiles"),
        ("modularity", "edgeCount"),
        ("godFile", "total"),
        ("siblingCross", "totalIntraModuleEdges"),
        ("diamond", "rootsExamined"),
        ("renameInstability", "total"),
        ("busFactor", "total"),
        ("fixRatio", "taggedFileTouches"),
    ];

    let scores = wire_scores();
    let scores = scores.as_object().expect("Scores is a JSON object");

    let declared: BTreeSet<String> = POPULATION_FIELD
        .iter()
        .map(|(k, _)| k.to_string())
        .collect();
    let wire: BTreeSet<String> = scores.keys().cloned().collect();
    assert_eq!(
        wire, declared,
        "every score must declare which field carries its population, and no declaration may \
             outlive its score — add the row to POPULATION_FIELD and to `Scores`' population table \
             together. A score whose population nobody can name is not a measurement."
    );

    for (key, population_field) in POPULATION_FIELD {
        let score = scores[*key]
            .as_object()
            .unwrap_or_else(|| panic!("`{key}` serializes as an object"));
        assert!(
            score.contains_key("score"),
            "`{key}` must carry a `score` — otherwise this pin is checking the wrong shape"
        );
        let population = score.get(*population_field).unwrap_or_else(|| {
                panic!(
                    "`{key}` ships a `score` but no `{population_field}`, so 100 means both \
                     'judged everything and found nothing' and 'found nothing to judge'. The \
                     denominator is the field that tells them apart and it must ride WITH the score \
                     — see `Scores`' population table."
                )
            });
        assert!(
            population.is_u64(),
            "`{key}.{population_field}` must be a plain count (got {population}) — a population \
                 is how many subjects were judged, never a rate or a nullable"
        );
    }
}

/// The completeness pin, in both directions, against serde's own output rather than a list written
/// by hand: a new score field turns this red, and a meaning for a field that no longer exists does
/// too. Working-agreements §5.5① — the subject set is derived, never authored.
#[test]
fn every_score_field_has_a_meaning() {
    let wire = wire_keys();
    let documented: BTreeSet<String> = SCORE_MEANINGS.iter().map(|(k, _)| k.to_string()).collect();
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

/// The colliding-acronym pin is GONE with its last subject, and this test records why rather than
/// leaving a silent deletion.
///
/// It once covered two entries. `sfc` left first: its sentence had to disown Vue's Single-File
/// Component because the KEY was the collision, and renaming the key to `fileSizeCompliance`
/// removed the collision at its source, so the disclaimer went with it. `lod` (Law of Demeter, NOT
/// Level of Detail) kept its half — until 2026-08-08, when the SCORE itself was removed for never
/// having measured anything (see [`crate::scores::types::Scores`]). A disclaimer about how to read
/// a number that no longer ships would be worse than none.
///
/// The completeness pin above is what keeps this honest in both directions: if `lod` ever returns
/// with a real producer, its sentence must return too, and this note is where to look for the
/// wording that was there before.
#[test]
fn no_meaning_survives_its_score() {
    for gone in ["lod", "typeSafety", "sfc", "fsd"] {
        assert_eq!(
            score_meaning(gone),
            None,
            "`{gone}` is not a score this build emits, so no sentence may explain it — a legend \
                 outliving its number is the stale half of the completeness pin above"
        );
    }
}
