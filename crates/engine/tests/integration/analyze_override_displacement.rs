//! e2e acceptance for `overrides`: the `examples/adapters/override-required/` tree, whose README
//! states this exact criterion.
//!
//! That tree is the committed measurement of the one thing additive merging cannot do. Two real files
//! answer one import name (`util/config.py` at the root, `src/util/config.py` in src-layout), path
//! candidates try the root first, and both spellings name a file that exists — so the native parser
//! emits a resolvable, deterministic, WRONG edge. Before `overrides`, an adapter that knew the right
//! target could only ADD its edge: the wrong one is native, native facts were never displaced, and the
//! run ended with two edges for one import and nothing saying so.
//!
//! Three things are pinned here, and the middle one is the reason the other two are not enough on their
//! own:
//!   1. the wrong native edge is GONE (displacement actually happened);
//!   2. the run SAYS a native fact was displaced, naming both sides — an override is the only overlay
//!      operation that removes something the engine extracted itself, so without this line the output
//!      is a graph that silently disagrees with what the engine read;
//!   3. the right edge is present and it is the ONLY one (the replacement landed, and displacement did
//!      not degenerate into deletion).
//!
//! The tree is read from `examples/` rather than rebuilt inline, so this test and the documented
//! example cannot drift into describing different trees.

use std::path::{Path, PathBuf};

use zzop_engine::{analyze_tree, EngineConfig};

const IMPORTER: &str = "src/app.py";
const WRONG_TARGET: &str = "util/config.py";
const RIGHT_TARGET: &str = "src/util/config.py";

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/adapters/override-required")
        .canonicalize()
        .expect(
            "the override-required example must exist — it is this contract's acceptance fixture",
        )
}

fn overlay() -> zzop_core::NormalizedEnvelope {
    let json = std::fs::read_to_string(fixture_root().join("overlay.json"))
        .expect("fixture overlay.json must be readable");
    zzop_core::validate_envelope(&json)
        .expect("the committed fixture overlay must satisfy the real contract validator")
}

/// Targets of every dep edge out of `src/app.py`, which is the whole graph this fixture is about.
fn targets_of_importer(out: &zzop_engine::AnalyzeOutput) -> Vec<String> {
    let mut targets = out.ir.ir.dep.get(IMPORTER).cloned().unwrap_or_default();
    targets.sort();
    targets
}

/// The BASELINE half, kept in this file so the acceptance criterion is legible without cross-reading:
/// natively, this tree produces one edge and it is the wrong one. If this ever stops holding, the
/// fixture has lost the property it exists for and the override assertions below become vacuous.
#[test]
fn natively_the_fixture_resolves_one_edge_and_it_is_the_wrong_one() {
    let out = analyze_tree(&fixture_root(), &EngineConfig::default());
    assert_eq!(
        targets_of_importer(&out),
        vec![WRONG_TARGET.to_string()],
        "the fixture's whole purpose is a resolvable-but-wrong native edge"
    );
}

/// THE ACCEPTANCE TEST. One correct edge, and the displaced native fact disclosed.
#[test]
fn a_declared_override_replaces_the_wrong_native_edge_and_discloses_what_it_displaced() {
    let config = EngineConfig {
        adapter_overlays: vec![overlay()],
        ..EngineConfig::default()
    };
    let out = analyze_tree(&fixture_root(), &config);

    assert_eq!(
        targets_of_importer(&out),
        vec![RIGHT_TARGET.to_string()],
        "expected exactly the corrected edge: the wrong native target must be displaced (not merely \
         joined by the right one), and the replacement must land"
    );

    let tombstone = out
        .warnings
        .iter()
        .find(|w| w.contains("DISPLACED"))
        .unwrap_or_else(|| {
            panic!(
                "the run must disclose that it dropped a natively-parsed fact — a graph that silently \
                 disagrees with what the engine read is the failure this contract exists to prevent. \
                 warnings: {:?}",
                out.warnings
            )
        });
    // Both sides, so the judgment can be re-derived and disagreed with. A bare count would say a
    // displacement happened without letting anyone check whether the adapter was the correct side.
    assert!(
        tombstone.contains(WRONG_TARGET) || tombstone.contains("util.config"),
        "the disclosure must name what was displaced: {tombstone}"
    );
    assert!(
        tombstone.contains("src.util.config"),
        "the disclosure must name what replaced it: {tombstone}"
    );
}

/// Deletion stays refused at the contract boundary, on the real fixture rather than a synthetic
/// envelope: strip the replacement binding and the same declaration becomes invalid, so the overlay is
/// rejected wholesale and the native fact survives untouched. An adapter cannot use `overrides` to make
/// the engine forget something and put nothing in its place.
#[test]
fn stripping_the_replacement_turns_the_override_into_a_refused_deletion() {
    let mut envelope = overlay();
    envelope.files[0].imports.clear();

    let json = serde_json::to_string(&envelope).expect("envelope is Serialize");
    let errors = zzop_core::validate_envelope(&json)
        .expect_err("an override with no replacement binding is a deletion and must not validate");
    assert!(errors.iter().any(|e| e.contains("deletion")), "{errors:?}");

    // And end-to-end: a rejected overlay contributes nothing, so the native (wrong) edge is still there
    // rather than the file losing its import entirely.
    let config = EngineConfig {
        adapter_overlays: vec![envelope],
        ..EngineConfig::default()
    };
    let out = analyze_tree(&fixture_root(), &config);
    assert_eq!(
        targets_of_importer(&out),
        vec![WRONG_TARGET.to_string()],
        "a refused overlay must leave the native graph exactly as it found it"
    );
}

/// THE MIRROR. Native-first is the default, and it is correct — but the adapter's dropped side must not
/// vanish in silence either, or an author who forgets (or misspells) the declaration gets a run that
/// looks exactly like success: their corrected binding absent, the wrong native one still in place, and
/// nothing separating "I was overruled" from "I was applied".
///
/// This is easy to hit on THIS tree, which is why it is pinned here: Python binds `import util.config`
/// under the local name `util`, so an adapter naming the key `util.config` misses the collision entirely
/// while one naming `util` — the correct key — collides and needs the declaration.
#[test]
fn an_undeclared_collision_keeps_the_native_binding_and_says_the_overlay_was_overruled() {
    let mut envelope = overlay();
    envelope.files[0].overrides = Default::default();
    // With no override left to declare, v1 is the honest version for these bytes (the gate only binds
    // envelopes that actually carry a declaration).
    envelope.version = zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string();

    let config = EngineConfig {
        adapter_overlays: vec![envelope],
        ..EngineConfig::default()
    };
    let out = analyze_tree(&fixture_root(), &config);

    assert_eq!(
        targets_of_importer(&out),
        vec![WRONG_TARGET.to_string()],
        "without a declaration the parsed fact wins — that is the default and it is not the bug"
    );
    let dropped = out
        .warnings
        .iter()
        .find(|w| w.contains("DROPPED"))
        .unwrap_or_else(|| {
            panic!(
                "the run must say the overlay's binding was overruled: {:?}",
                out.warnings
            )
        });
    assert!(
        dropped.contains("src.util.config"),
        "the disclosure must name what was dropped: {dropped}"
    );
    assert!(
        dropped.contains("overrides.imports"),
        "and point at the fix, since this is usually a missing declaration: {dropped}"
    );
}

/// An overlay that RESTATES a binding the native pass already holds is agreement, not a loss, and must
/// stay silent — otherwise every subset adapter (`java-imports-adapter` is exactly this) would warn on
/// every run, which is how a real signal gets trained away.
#[test]
fn an_overlay_restating_the_native_binding_reports_nothing() {
    let mut envelope = overlay();
    envelope.files[0].overrides = Default::default();
    envelope.version = zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string();
    // Agree with the native parser instead of correcting it.
    for binding in envelope.files[0].imports.values_mut() {
        binding.specifier = "util.config".to_string();
    }

    let config = EngineConfig {
        adapter_overlays: vec![envelope],
        ..EngineConfig::default()
    };
    let out = analyze_tree(&fixture_root(), &config);
    assert!(
        !out.warnings.iter().any(|w| w.contains("DROPPED")),
        "agreement is not a loss: {:?}",
        out.warnings
    );
}
