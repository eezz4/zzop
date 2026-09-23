use super::types::{ANALYSIS_DARK, EXTRACTION_BLIND, INPUT_CONFIG, TRUST_CALIBRATION};
use super::*;
use std::collections::BTreeSet;

/// The pin: the EXACT `(id, status)` map every registered class must carry. Pinning the STATUS (not
/// just the id) is the honesty guard — this registry's whole point is to not overclaim, so an
/// aspirational flip of any class to a stronger status (e.g. `provide-side-unextracted` ->
/// `notYetDetected` promoted to `asserted` before the detection actually ships) MUST fail the gate
/// rather than pass silently. Adding/renaming/removing a class, or changing any status, fails here —
/// update this table deliberately, in lock-step with the real shipped detection.
///
/// WHAT THIS PIN IS DESIGNED TO MISS, written here so its green is never cited as coverage: both sides
/// of every row are authored by hand, so it proves a status did not CHANGE unnoticed and nothing
/// whatever about whether the status is TRUE. A row can name a mechanism that was since narrowed, or
/// omit one that since shipped, and this table stays green. Measured 2026-08-29, when an outside audit
/// put a counter-example to each of the six `asserted` rows and three did not survive — every one of
/// the three sat under this green pin. What closes that gap is not here but BESIDE each row:
/// `every_asserted_row_names_a_firing_pin_that_exists_and_names_it_back` requires every `asserted` row
/// to name a test that runs its claim, and requires that test to name the row back. This table's own
/// job stays what it was — a status may not move unnoticed — and its green is not evidence of anything
/// more.
const EXPECTED: &[(&str, &str)] = &[
    ("capability-absent-vs-empty", "asserted"),
    ("channel-empty-family-dark", "partial"),
    ("classified-skip", "partial"),
    ("coincidental-match", "asserted"),
    ("config-error", "asserted"),
    // Demoted asserted -> partial 2026-08-29, the reverse of every other movement in this table, and
    // the reason is that the row was reading its ASSERTED FACT as if it covered its CLASS.
    // `coverage.joinContributionZero` really is emitted every run — but its predicate is conjunctive
    // (files > 0 AND zero provides AND zero keyed consumes), so a tree that publishes routes can never
    // satisfy it no matter how dark its consume channel goes. Measured on a 717-file Java tree: 322
    // provides, 0 keyed consumes, `joinContributionZero` false, no warning about the consume side at
    // all, and two files in the tree driving an http client directly. What remains beside the fact is
    // three near-zero tree-wide heuristics, which is the definition of `partial`. The mirror row
    // `provide-side-unextracted` describes the same loss on the other channel, has strictly MORE
    // machinery for it (a per-FILE gate this side has no twin of) and has said `partial` since it
    // shipped — so the pair was labelled backwards from its own mechanisms, which is what
    // `mirrored_channel_rows_carry_the_same_status` below now refuses.
    ("consume-side-unextracted", "partial"),
    ("generated-client-unrecognized", "partial"),
    ("input-scope-error", "partial"),
    ("join-bucket-unfiltered", "notYetDetected"),
    ("key-mismatch-drift", "partial"),
    ("language-unparsed", "partial"),
    // Promoted notYetDetected -> partial 2026-08-17. Not because the engine can verify an overlay's
    // facts — it still cannot — but because the reader could not previously tell which facts were an
    // overlay's AT ALL. Two measured consequences of that: a successful run named the adapter's
    // `parser` id nowhere, and the framework-silence tripwire went quiet once the overlay supplied
    // routes, so writing the adapter deleted the warning that asked for one. Every run an overlay
    // contributed to now names each parser, its counts, and the share of http routes that were declared
    // rather than extracted, and the tripwires judge on the extracted half. Still `partial`, never
    // `asserted`: the disclosure is per-RUN, not per-fact — an individual provide still carries no
    // origin, so a wrong key inside a well-formed overlay is as invisible as before.
    ("overlay-facts-unverified", "partial"),
    ("provide-side-unextracted", "partial"),
    ("resolution-gap", "asserted"),
    // Promoted notYetDetected -> partial when the call-graph LANGUAGE gap became a real per-run
    // self-report (`framework_silence::call_graph_language_gap_warning`, tripwire S8): a tree with http
    // routes in a language that has no call-site producer now names that language and the rule it
    // silences. Since then `zzop coverage`'s `trees[].blindSpots` lists the empty-evidence-channel
    // rules per tree for every DECLARED sightline (several pairs, derived from compiled-in rule
    // metadata). Still `partial`, never `asserted` — one opt-in CLI lane, a subset of rules: the
    // analyze reply itself carries no such field, and undeclared rules stay prose-only.
    ("rule-evidence-language-gap", "partial"),
    // Added 2026-08-08. The scoring lane had NO class at all, while every per-metric formula returned
    // 100 on an empty population — so a metric with nothing to measure was byte-identical to a clean
    // one, and `health.pain` (which dropped zero-contributors) read strictly BETTER for a tree zzop
    // could see less of. Measured: `typeSafety`/`lod` scored 100 on every run ever made (no producer
    // for either input channel anywhere in the build), `mainSequence` computed abstractness 0 for
    // every module, and `featureSlicedDesign` scored a Go tree 0 purely because gRPC-Gateway puts
    // `api/` at the BOTTOM while FSD reads that name as the TOP entry layer.
    //
    // `asserted` from the start, which is unusual for a new class and is earned by the SHAPE rather
    // than by a detector: the population is a field on every score object, produced by the same
    // computation as the number beside it, and a derived test rejects any score that ships without
    // one. There is no run in which the signal can be absent, so `partial` would understate it. What
    // the class explicitly does NOT claim is whether a non-zero population is representative — that
    // residual is named in the summary rather than folded into the status.
    ("score-population-empty", "asserted"),
    ("silent-truncation", "partial"),
    ("stale-cache", "partial"),
];

#[test]
fn registry_matches_the_pinned_id_and_status_map() {
    let actual: BTreeSet<(&str, &str)> = blindness_registry()
        .iter()
        .map(|c| (c.id, c.status.as_str()))
        .collect();
    let expected: BTreeSet<(&str, &str)> = EXPECTED.iter().copied().collect();
    assert_eq!(
        actual, expected,
        "the blindness registry drifted from its pinned (id, status) map — a class was added, \
         renamed, removed, or (crucially) had its status changed. Update EXPECTED deliberately, and \
         only promote a status once the real detection ships (never aspirationally)."
    );
    // No duplicate ids (the BTreeSet would swallow a dup on `id` only if statuses also matched, so
    // check the raw count too).
    assert_eq!(blindness_registry().len(), EXPECTED.len());
}

#[test]
fn every_group_is_valid_and_all_four_are_represented() {
    let valid = [
        EXTRACTION_BLIND,
        ANALYSIS_DARK,
        INPUT_CONFIG,
        TRUST_CALIBRATION,
    ];
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for class in blindness_registry() {
        assert!(
            valid.contains(&class.group),
            "unknown group {:?} on {:?}",
            class.group,
            class.id
        );
        assert!(
            !class.summary.trim().is_empty(),
            "empty summary on {:?}",
            class.id
        );
        seen.insert(class.group);
    }
    assert_eq!(
        seen.len(),
        valid.len(),
        "not all four taxonomy groups are represented"
    );
}

#[test]
fn status_tokens_are_the_three_known_camel_case_values() {
    for class in blindness_registry() {
        assert!(
            matches!(
                class.status.as_str(),
                "asserted" | "partial" | "notYetDetected"
            ),
            "unexpected status token {:?}",
            class.status.as_str()
        );
    }
}

/// RED at `dd4db4d`: **a mirrored pair may not disagree about its own status.**
///
/// `consume-side-unextracted` and `provide-side-unextracted` are one question asked on the two sides of
/// the same join, and they are the only rows in this registry whose ids differ by nothing but that
/// word. They had drifted to `asserted` / `partial` — and the side carrying the STRONGER label was the
/// side with the WEAKER mechanism: the provide row had gained a per-FILE gate (a framework-importing
/// file that contributed no route) while the consume row still rested on a tree-wide conjunction that a
/// route-serving tree can never satisfy. Nothing noticed, because the pin above compares each row only
/// with a copy of itself.
///
/// The pairing is DERIVED from the ids rather than listed, so a future `consume-side-*`/`provide-side-*`
/// pair is judged the day it lands. If the two channels genuinely diverge one day — one side really
/// does gain a mechanism the other lacks — that is a deliberate claim, and it belongs in this test as a
/// named exception carrying which side is stronger and why, never as a quiet edit to `EXPECTED`.
#[test]
fn mirrored_channel_rows_carry_the_same_status() {
    let by_id: std::collections::BTreeMap<&str, &str> = blindness_registry()
        .iter()
        .map(|c| (c.id, c.status.as_str()))
        .collect();

    let mut pairs = 0usize;
    for (id, status) in &by_id {
        if !id.contains("consume") {
            continue;
        }
        let mirror = id.replace("consume", "provide");
        let Some(mirror_status) = by_id.get(mirror.as_str()) else {
            continue;
        };
        pairs += 1;
        assert_eq!(
            status, mirror_status,
            "`{id}` says {status} while its mirror `{mirror}` says {mirror_status}. These are the same \
             question on the two sides of one join; a difference here is a claim that one channel is \
             detected better than the other, which has to be true of the MECHANISMS and stated out \
             loud, not left as a status drift."
        );
    }

    assert!(
        pairs >= 1,
        "no mirrored consume/provide row pair was found — the id spelling this derivation keys on \
         changed, so this test is now measuring nothing (a green that means the guard broke, not that \
         the registry is sound)."
    );
}

/// **An `asserted` row must say, in the source beside it, why it cannot be silently missed.**
///
/// `asserted` is the one status that promises something unconditional, and it is the status an author
/// reaches for when a mechanism looks solid from the inside. The 2026-08-29 audit is the measurement
/// behind this test: of the six rows carrying it, the ONE that survived a counter-example attempt
/// (`score-population-empty`) was also the only one whose author had written down what made the label
/// hold — "the population is a field on every score object, produced by the same computation as the
/// number beside it". The three that did not survive had no such sentence anywhere.
///
/// This does NOT verify the label; prose proves nothing on its own. What it does is make the label's
/// evidence a thing that exists in a fixed place — the next promoter has to produce it, and the next
/// auditor has one paragraph to attack instead of a mechanism to go find. The residual is named rather
/// than papered over: a wrong reason passes this test exactly as a right one does.
///
/// The subject set is scanned out of the registry source, and the scan's own count is cross-checked
/// against the live registry so a broken scan cannot pass by finding nothing.
#[test]
fn every_asserted_row_states_why_it_cannot_be_silently_missed() {
    let rows = asserted_rows_in_source();
    let unjustified: Vec<&str> = rows
        .iter()
        .filter(|r| r.reason_lines < 2)
        .map(|r| r.label.as_str())
        .collect();
    assert!(
        unjustified.is_empty(),
        "`asserted` row(s) with no reason written beside them: {unjustified:?}. `asserted` promises a \
         signal that cannot be absent on any run — write, immediately above the `status:` line, WHAT \
         carries it unconditionally and what the class deliberately does not claim. Every other status \
         in this registry that was argued for carries such a block; the rows that did not are the ones \
         an outside audit broke."
    );
}

/// **An `asserted` row must name a TEST that runs its claim, and that test must name the row back.**
///
/// This is the check the pin at the top of this file says it is not: that pin compares each row with a
/// hand-written copy of itself, and the prose gate above accepts a wrong reason exactly as it accepts a
/// right one. Both were green on 2026-08-29 while an outside audit put a counter-example to three of
/// the six `asserted` rows. A label nothing executes is this repo's "green is not what you think it
/// is" shape, and `asserted` is the worst place to leave it: a disclosure registry is the canonical
/// answer to what this tool cannot see.
///
/// So every `asserted` row carries `// FIRING PIN: <repo-relative path>::<test fn>` in the comment
/// block above its `status:` line, and this test walks that pin FROM BOTH ENDS — the shape
/// `crates/summary/tests/legend_fold.rs` already uses on the folded legends:
///
/// * the named file exists in this checkout,
/// * it holds a `fn <name>(`, and
/// * it names the CLASS ID verbatim — which is what makes the pin un-fakeable by pointing at whatever
///   test happened to be nearby. Pin and test now have to be edited together or one of them goes red.
///
/// What this still does NOT do, written here so its green is never cited as more: it does not read the
/// assertions inside the pinned test, so a test that names the class and measures something adjacent
/// to it passes. It moves the failure from "nobody ever ran this label" to "somebody had to write a
/// run shaped like it and say which label it is", and it makes a class that loses its mechanism go red
/// in the test that lost it instead of staying green here.
#[test]
fn every_asserted_row_names_a_firing_pin_that_exists_and_names_it_back() {
    let rows = asserted_rows_in_source();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let mut resolved = 0usize;
    let mut problems: Vec<String> = Vec::new();
    for row in &rows {
        if row.pins.is_empty() {
            problems.push(format!(
                "{} (`{}`) carries no `// FIRING PIN: <path>::<test fn>` line",
                row.label, row.id
            ));
            continue;
        }
        for pin in &row.pins {
            let Some((rel, func)) = pin.rsplit_once("::") else {
                problems.push(format!(
                    "{} (`{}`): pin {pin:?} is not `<repo-relative path>::<test fn>`",
                    row.label, row.id
                ));
                continue;
            };
            let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
                problems.push(format!(
                    "{} (`{}`): pin names `{rel}`, which this checkout does not have",
                    row.label, row.id
                ));
                continue;
            };
            if !text.contains(&format!("fn {func}(")) {
                problems.push(format!(
                    "{} (`{}`): `{rel}` holds no `fn {func}(` — the pin points at a test that was \
                     renamed or removed",
                    row.label, row.id
                ));
            } else if !text.contains(row.id.as_str()) {
                problems.push(format!(
                    "{} (`{}`): `{rel}` never names `{}`, so the pin is one-ended — a test that does \
                     not say which disclosure class it measures cannot be audited as measuring it",
                    row.label, row.id, row.id
                ));
            } else {
                resolved += 1;
            }
        }
    }

    assert!(
        problems.is_empty(),
        "`asserted` is the one status that promises something unconditional, and these rows have no \
         run standing behind them: {problems:#?}"
    );
    assert!(
        resolved >= rows.len(),
        "{resolved} pin(s) resolved for {} `asserted` row(s) — this guard would have passed while \
         proving less than one run per row, which is the vacuous green it exists to refuse.",
        rows.len()
    );
}

/// One `asserted` row as the registry SOURCE spells it — the subject set both meta tests judge.
struct AssertedRow {
    /// `<file>:<line>` of the `status:` line, so a failure names the row a reader has to open.
    label: String,
    /// The row's own `id`, read from the nearest `id: "…"` line above it.
    id: String,
    /// Comment lines in the contiguous block immediately above `status:`.
    reason_lines: usize,
    /// Every `FIRING PIN:` payload in that block, in source order.
    pins: Vec<String>,
}

/// Scan the registry source for `asserted` rows, cross-checking the scan's own count against the live
/// registry so a broken scan (a moved file, a reformatted `status:` line) cannot pass by finding
/// nothing — the vacuous green this repo has now measured several times.
fn asserted_rows_in_source() -> Vec<AssertedRow> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/disclosure/registry");
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));

    let mut rows: Vec<AssertedRow> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.trim() != "status: DisclosureStatus::Asserted," {
                continue;
            }
            let block: Vec<&str> = lines[..i]
                .iter()
                .rev()
                .take_while(|l| l.trim_start().starts_with("//"))
                .copied()
                .collect();
            let id = lines[..i]
                .iter()
                .rev()
                .find_map(|l| l.trim_start().strip_prefix("id: \""))
                .and_then(|rest| rest.split('"').next())
                .unwrap_or_else(|| panic!("no `id:` line above {file}:{}", i + 1))
                .to_string();
            // Anchored at the start of the comment line, not a substring search: this block is prose
            // as well as machine input, and a sentence ABOUT the pin convention is not a pin. Caught
            // by this very test on 2026-09-11, when a comment two rows down explaining the convention
            // was read as a malformed pin.
            let mut pins: Vec<String> = block
                .iter()
                .filter_map(|l| l.trim_start().strip_prefix("// FIRING PIN:"))
                .map(|rest| rest.trim().to_string())
                .collect();
            pins.reverse();
            rows.push(AssertedRow {
                label: format!("{file}:{}", i + 1),
                id,
                reason_lines: block.len(),
                pins,
            });
        }
    }

    let asserted = blindness_registry()
        .iter()
        .filter(|c| c.status == DisclosureStatus::Asserted)
        .count();
    assert_eq!(
        rows.len(),
        asserted,
        "the source scan found {} `asserted` row(s) while the registry holds {asserted} — the scan is \
         reading a different set than it is judging (a moved file, a reformatted `status:` line), so \
         every verdict built on it is about nothing.",
        rows.len()
    );
    rows
}
