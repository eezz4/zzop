//! Pins for [`super::SCORE_MEANINGS`] and for the population contract every score must satisfy.
//!
//! Split out of the parent on 2026-08-08 to stay under the per-file line cap when the
//! `every_score_ships_the_population_it_scored_over` pin landed. Every pin here derives its subject
//! set rather than authoring it — from serde's own output for the two wire-shape pins, and from the
//! engine's own prose for the disclosure pin — which is what makes them survive a new score, or a new
//! sentence, added by someone who never reads this file.

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

/// The population field for each score key — the wire spelling of the mapping
/// `crate::health::population_of` reads. Most metrics name their own ratio denominator;
/// `mainSequence` is a DISTANCE rather than a ratio, so it names its classified population
/// instead, and `featureSlicedDesign` names a second count (`layerClassifiedImports`) rather than
/// its ratio denominator (`totalImports`), which counts imports it could not classify.
/// (`sdp` IS a ratio — `sdp.rs` scores `bad / total_cross_slice_edges` — and named itself
/// distance-shaped here until 2026-08-14.)
///
/// Module-scoped rather than declared inside the pin below since 2026-08-14: two pins now read it, and
/// the second one (`every_score_field_the_disclosure_prose_names_is_that_scores_population`) exists
/// precisely because a SECOND copy of this mapping, written in prose, had drifted.
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

/// RED (defects b and c): **no score may ship without its population.**
///
/// Same derivation discipline as the meanings pin below — the subject set is serde's own output, so
/// a sixteenth score cannot land without answering "over what?". The population field per metric is
/// declared once, in [`POPULATION_FIELD`] above, and this crosses that declaration
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
             outlive its score — add the row to POPULATION_FIELD and the arm to \
             `crate::health::population_of` together. A score whose population nobody can name is \
             not a measurement."
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
                     — see POPULATION_FIELD above and `Scores`' own doc for why."
                )
            });
        assert!(
            population.is_u64(),
            "`{key}.{population_field}` must be a plain count (got {population}) — a population \
                 is how many subjects were judged, never a rate or a nullable"
        );
    }
}

/// The engine's blindness-registry sources, as TEXT with Rust's line continuations already joined.
///
/// Read as text rather than imported, because no import can run this way: `zzop-engine` DEPENDS on
/// this crate, so the mapping being compared is a Rust value HERE and an English sentence THERE, and a
/// file read is the only direction available. Same shape as
/// `crates/engine/tests/rule_contracts/*`'s cross-crate pins, pointed the other way.
///
/// The continuation join is load-bearing, not tidying: the registry's summaries are `\`-continued
/// string literals, so `featureSlicedDesign.\n  layerClassifiedImports` is one token to a reader and
/// two to a naive scan — a rewrap could otherwise hide a wrong field from this pin.
fn disclosure_prose() -> Vec<(String, String)> {
    let engine_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../engine/src");
    let join_continuations = regex::Regex::new(r"\\\r?\n[ \t]*").expect("static regex");
    let read = |path: &std::path::Path, label: &str| -> String {
        let raw = std::fs::read_to_string(path).unwrap_or_else(|e| {
            panic!(
                "cannot read {label} ({e}) — this pin judges the population fields the blindness \
                 registry NAMES in prose, so a registry that moved leaves the claim unjudged. Point \
                 this path at the registry's new home; do not delete the pin."
            )
        });
        join_continuations.replace_all(&raw, "").into_owned()
    };

    let root = engine_src.join("disclosure.rs");
    let mut sources = vec![(
        "crates/engine/src/disclosure.rs".to_string(),
        read(&root, "crates/engine/src/disclosure.rs"),
    )];
    assert!(
        sources[0].1.contains("BLINDNESS_REGISTRY"),
        "crates/engine/src/disclosure.rs no longer declares BLINDNESS_REGISTRY — this pin is reading \
         the wrong file and would go green on prose nobody ships"
    );
    // The module was split once already (`disclosure/document.rs`, `disclosure/types.rs`), so the
    // sibling directory is swept too rather than named file by file.
    if let Ok(entries) = std::fs::read_dir(engine_src.join("disclosure")) {
        let mut paths: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "rs"))
            .collect();
        paths.sort();
        for path in paths {
            let label = format!(
                "crates/engine/src/disclosure/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            let text = read(&path, &label);
            sources.push((label, text));
        }
    }
    sources
}

/// RED on 2026-08-14: **a score field the disclosure NAMES must be that score's population field.**
///
/// `score-population-empty` is the one blindness class that ships as `Asserted`, and what it asserts is
/// that a population rides every score — illustrated by naming three of them. The first example named
/// `featureSlicedDesign.totalImports`, which the field's own doc calls "the RATIO denominator …
/// Deliberately NOT the population", while `health::population_of` reads
/// `layerClassifiedImports`. So the sentence that promises "the denominator is there" pointed a reader
/// at the number that is large exactly when the metric judged nothing — the failure it was written to
/// disclose.
///
/// The sibling pin above cannot see this: it crosses the declaration against the WIRE, and the wire
/// carries both fields, so it stays green while the prose lies. The subject set here is the prose's own
/// `<scoreKey>.<field>` tokens, so a fourth example added tomorrow is judged without anyone editing
/// this file. Reading rule for future authors: inside the registry's prose, a `<scoreKey>.<field>`
/// token IS a population claim — a sentence that needs to name some other field of a score must teach
/// this pin the exception out loud rather than quietly weaken it.
#[test]
fn every_score_field_the_disclosure_prose_names_is_that_scores_population() {
    let token =
        regex::Regex::new(r"([a-z][A-Za-z0-9]*)\.([a-z][A-Za-z0-9]*)").expect("static regex");
    let mut judged = 0usize;

    for (label, text) in disclosure_prose() {
        for caps in token.captures_iter(&text) {
            let (key, field) = (&caps[1], &caps[2]);
            let Some((_, declared)) = POPULATION_FIELD.iter().find(|(k, _)| *k == key) else {
                continue;
            };
            judged += 1;
            assert_eq!(
                field, *declared,
                "{label} names `{key}.{field}`, but `{key}`'s population is `{declared}` — the \
                 disclosure would send a reader to a count that does not answer \"did this metric \
                 judge anything\". A ratio denominator is not a population: it can be large on the \
                 very tree the metric could not judge at all."
            );
        }
    }

    assert!(
        judged >= 3,
        "found only {judged} `<scoreKey>.<field>` token(s) in the blindness registry — the \
         `score-population-empty` class named three when this pin landed, and a pin with no subjects \
         passes by measuring nothing. If the prose deliberately stopped naming examples, lower this \
         floor in the same commit that removes them."
    );
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
