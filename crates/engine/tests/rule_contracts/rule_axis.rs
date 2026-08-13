//! The defect/opinion axis every SHIPPED DSL rule must declare (`RuleDef::axis`, added 2026-08-12).
//!
//! The axis answers "is this a bug report or a style note", and the pack format had no way to say it
//! until a user put the gap in one sentence: *"barrel-export discipline is not a rule, it's a house
//! convention."* `severity` does not answer it — see [`the_severity_band_does_not_reproduce_the_axis`],
//! which measures that the two disagree on real rules rather than asserting they must.
//!
//! Everything here reads the pack JSON **TEXT**, never the parsed value. `RuleDef::axis` carries
//! `#[serde(default)]` so third-party packs keep loading, and that default is invisible after parsing:
//! a rule that says nothing and a rule that says `"axis": "defect"` are the same `RuleAxis::Defect`
//! afterwards. Only the text can tell a DECLARATION from a silence, and "every shipped rule declared"
//! is precisely the claim being made.
//!
//! ## Why the subject is BUNDLED ∪ EXPORTED, since 2026-08-12
//!
//! [`every_shipped_rule_declares_its_axis`] read `rules/dsl/**` alone until `examples/packs/`'s
//! `typescript` pack — twelve rules that predate the field — was measured to declare it on NONE of them.
//! Those twelve were not "missing an axis": the enum's `#[default]` made them load as `defect`, silently
//! and indistinguishably from a deliberate declaration, and the repo's own prose got that backwards TWICE
//! in two days (`examples/packs/README.md`, fixed by `c0cc8ed`; `docs/rules/catalog.md`, fixed by
//! `590dd8d`) because nothing on either side of the sentence could be read off a machine.
//!
//! Export is not deletion — an exported pack is compiled into this binary as an `example-pack-*`
//! contract resource, served by `zzop contract`, explained by `zzop explain --config`, and it RUNS the
//! moment a tree's `zzop/rules/` holds it (the reasoning [`crate::load_shipped_packs`] states at length).
//! So a rule leaving the bundle must not be a way to stop declaring what kind of claim it makes.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use crate::{dsl_dir, exported_packs_dir};

/// Every shipped pack JSON file — one per `rules/dsl/<pack>/` directory, plus every `examples/packs/*.json`.
///
/// Deliberately NOT `load_dsl_packs`: this file's whole subject is what the TEXT says, so it needs paths
/// rather than parsed packs. `examples/packs/` is walked non-recursively — its `tests/` subdirectory holds
/// Rust, not packs.
fn shipped_pack_files() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut bundled: Vec<PathBuf> = fs::read_dir(dsl_dir())
        .expect("rules/dsl is readable")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .filter_map(|dir| {
            let name = dir.file_name()?.to_str()?.to_string();
            let path = dir.join(format!("{name}.json"));
            path.is_file().then_some(path)
        })
        .collect();
    bundled.sort();

    let mut exported: Vec<PathBuf> = fs::read_dir(exported_packs_dir())
        .expect("examples/packs is readable")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    exported.sort();

    (bundled, exported)
}

/// Every `<pack id>/<rule id>` in one pack file, paired with whether its JSON text declares `axis`.
///
/// Parsed off the text with the same indent anchors the packs are formatted to (the indentation
/// `check-deploy-facts-prose` already depends on): the pack id at two spaces, each rule id at six. So
/// this needs no JSON parser and cannot be fooled by a serde default. The pack id is read from the file
/// rather than from its directory or file NAME, because an exported pack keeps the id it had inside the
/// bundle while its file is named for the pack it ships as (`typescript-lint.json` holds pack
/// `typescript`) — a name-derived label would report ids no user can type.
fn declared_axes_in(path: &PathBuf) -> Vec<(String, Option<String>)> {
    let Ok(source) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = source.lines().map(str::trim_end).collect();
    let pack_id = lines
        .iter()
        .find_map(|l| l.strip_prefix(r#"  "id": ""#)?.strip_suffix(r#"","#))
        .unwrap_or_else(|| {
            panic!(
                "no pack id at the 2-space `\"id\":` anchor in {} — every rule below it would be \
                 labelled by a name no user can type. Re-point the anchor at whatever the packs are \
                 formatted to now.",
                path.display()
            )
        });
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix(r#"      "id": ""#) else {
            continue;
        };
        let Some(rule_id) = rest.strip_suffix(r#"","#) else {
            continue;
        };
        // The declaration, if any, is the line the stamper writes directly under the id.
        let axis = lines
            .get(i + 1)
            .and_then(|l| l.trim().strip_prefix(r#""axis": ""#))
            .and_then(|l| l.strip_suffix(r#"","#))
            .map(str::to_string);
        out.push((format!("{pack_id}/{rule_id}"), axis));
    }
    out
}

/// Extraction floors — a broken anchor, or a leg of the walk pointed at nothing, would make every
/// assertion below vacuously green.
///
/// One floor PER LEG rather than one on the total, because the two fail differently and the total hides
/// it: the bundle is two thirds of the corpus, so an `examples/packs/` walk that stopped finding files
/// would still clear any total floor a shrinking bundle can be trusted under. Each floor sits far below
/// today's count (116 bundled, 28 exported) — its job is structural breakage, not ordinary rule removal.
fn axes() -> Vec<(String, Option<String>)> {
    let (bundled, exported) = shipped_pack_files();
    let mut out = Vec::new();
    for (label, files, floor) in [
        ("rules/dsl", bundled, 100usize),
        ("examples/packs", exported, 20),
    ] {
        let mut leg = Vec::new();
        for path in &files {
            leg.extend(declared_axes_in(path));
        }
        assert!(
            leg.len() >= floor,
            "extraction floor: the 6-space `\"id\":` anchor found only {} rule(s) across {} {label} pack \
             file(s), under a floor of {floor}. Every check in this file would then pass by describing an \
             almost-empty set. Re-point the anchor at whatever the packs are formatted to now, or lower \
             the floor in the same commit if the shrink is deliberate.",
            leg.len(),
            files.len()
        );
        out.extend(leg);
    }
    out
}

/// No SHIPPED rule may rely on the serde default: each one states which kind of claim it makes.
///
/// The default exists for THIRD-PARTY packs (a pack written before this field must keep loading), and
/// it defaults to `defect` deliberately — a rule that forgot should read as the stronger claim and get
/// argued down, never quietly demoted to "just an opinion". Inside this repo the default is a way to
/// forget, so it is forbidden — in `examples/packs/` exactly as in `rules/dsl/`, see this file's header
/// for the twelve rules that proved the two halves needed the same rule.
///
/// This is the ROOT CAUSE needle, deliberately preferred over holding the prose that got it wrong: a
/// sentence about which packs declare the axis has to be attached to a Markdown section by inference,
/// and its failure mode is attaching to the wrong set silently. "No rule inherits the default" needs no
/// inference and is strictly stronger — it is the thing that made the wrong sentences possible.
#[test]
fn every_shipped_rule_declares_its_axis() {
    let all = axes();
    let missing: Vec<&str> = all
        .iter()
        .filter(|(_, axis)| axis.is_none())
        .map(|(id, _)| id.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "these shipped rules declare no `axis` and would silently inherit `defect` — say which kind of \
         claim each makes (`\"axis\": \"defect\"` or `\"axis\": \"opinion\"`, on the line under `\"id\"`): \
         {missing:#?}"
    );
}

/// Only the two spellings the enum accepts, caught here rather than at load time.
///
/// A typo (`"opnion"`) is not a load failure: `#[serde(default)]` does not make an unknown VALUE
/// default, serde rejects it — but a pack that fails to parse is skipped by `explain`'s best-effort
/// loader, so the misspelled rule would simply vanish from that surface with no message.
#[test]
fn every_declared_axis_is_a_spelling_the_enum_accepts() {
    let bad: Vec<String> = axes()
        .iter()
        .filter_map(|(id, axis)| {
            axis.as_deref()
                .filter(|a| !matches!(*a, "defect" | "opinion"))
                .map(|a| format!("{id}: {a:?}"))
        })
        .collect();
    assert!(bad.is_empty(), "unknown axis spellings: {bad:#?}");
}

/// The measurement this axis exists for, held as a test so it cannot rot into a claim nobody rechecks:
/// **`severity` does not reproduce the defect/opinion split**, in BOTH directions.
///
/// This was the meeting's decisive evidence against "just read severity". The `info` band holds real
/// defects (an unauthenticated mutating route) beside pure taste (`select-star`), and the `warning`
/// band holds opinions. Severity encodes CONFIDENCE x BLAST RADIUS — a different question, whose answer
/// is allowed to differ, and does.
///
/// Asserted as "the two partitions are not equal" rather than by naming rules, so re-severitying any
/// individual rule is free; only a table where severity HAS become the axis turns this red, and then
/// the axis field is redundant and should be argued about rather than kept out of habit.
///
/// ⚠ **This is the one test in this file that reads the exported packs through the PARSED loader**
/// ([`load_shipped_packs`]) rather than off their text, and the reason is worth a line so the next reader
/// does not "simplify" it back. On 2026-08-12 the last
/// `axis: opinion` rules left the bundle, and this assertion went red with a message telling its reader
/// to delete the test. That message was wrong for this case: it was written against *"every opinion rule
/// was deleted"*, and what happened instead is that every opinion rule was EXPORTED. The measurement —
/// severity does not reproduce the defect/opinion split — is a property of this build's rule corpus, and
/// an exported rule is still in it: still shipped, still loadable, still firing under its own id once
/// recovered. Deleting the test would have recorded a change of address as a loss of evidence. Its two
/// siblings above reached the same population later the same day and by the same argument — see this
/// file's header — so what still separates them is the READING, not the subject: they need the text
/// (a serde default is invisible after parsing), this one needs the parsed `severity`/`axis` pair.
#[test]
fn the_severity_band_does_not_reproduce_the_axis() {
    let packs = crate::load_shipped_packs();
    let mut opinion_severities: BTreeSet<String> = BTreeSet::new();
    let mut defect_severities: BTreeSet<String> = BTreeSet::new();
    let mut opinion_count = 0usize;
    for pack in &packs {
        for rule in &pack.rules {
            let sev = format!("{:?}", rule.severity);
            match rule.axis {
                zzop_core::RuleAxis::Opinion => {
                    opinion_count += 1;
                    opinion_severities.insert(sev);
                }
                zzop_core::RuleAxis::Defect => {
                    defect_severities.insert(sev);
                }
            }
        }
    }
    assert!(
        opinion_count > 0,
        "no SHIPPED rule declares `opinion` — bundled or exported. Note what this does NOT mean: the \
         bundle going to zero opinion rules is expected (it did, on 2026-08-12) and this loader already \
         covers that case by reading `examples/packs/` too. Red here means the axis has no subject \
         ANYWHERE in the corpus, so either the field is dead and should be argued about, or the \
         exported-pack leg of `load_shipped_packs` stopped seeing its directory"
    );
    let overlap: Vec<&String> = opinion_severities
        .intersection(&defect_severities)
        .collect();
    assert!(
        !overlap.is_empty(),
        "severity now partitions the axis exactly (no band holds both kinds): {opinion_severities:?} \
         vs {defect_severities:?}. That makes `axis` derivable from `severity`, so either the axis \
         field is redundant or a rule was mis-severitied — decide which, do not just re-pin this."
    );
}
