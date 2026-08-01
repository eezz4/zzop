//! Binds the Normalized-AST ENVELOPE contract version to every copy of it that ships.
//!
//! `zzop_core::NORMALIZED_AST_CONTRACT_VERSION` has an unusual contract, stated at its declaration as a
//! 2026-07-31 user ruling: it moves ONLY when the envelope shape moves, explicitly not every release —
//! *"bumping it every release would be the defect this replaces — a number that appears to describe the
//! shape while actually describing the calendar."*
//!
//! That makes it the one semver-shaped string in this repo that must NOT track the workspace version,
//! and it is spelled in four places that ship: the constant, the example envelope, the JSON schema's
//! `description`, and `docs/NORMALIZED_AST.md`'s "emit this" instruction. All four are baked into both
//! binaries via `crates/summary`'s `include_str!` contract docs, so a disagreement is not a doc bug —
//! it is `zzop contract example-envelope` and `zzop contract envelope-guide` telling one adapter author
//! two different things.
//!
//! ## The failure this exists to prevent, which already happened
//! During the v0.28.0 bump, `scripts/check-release-version-propagation.sh` (semver-shaped, and unable to
//! know this field is not release-tracking) rewrote `example-envelope.json` to `0.28.0`, and the constant
//! was moved with it — for a release whose only envelope diff was a prose `description` rewrite. The
//! schema and the markdown still said `0.27.0`, correctly. An adapter author copying the shipped example
//! would have emitted an envelope that every 0.27.x engine REJECTS ("reject newer, never guess") for a
//! shape byte-identical to the one they already had. Caught in that release's pre-tag audit by reading;
//! nothing mechanical would have caught it, which is why this file exists.
//!
//! Deliberately NOT checked here: whether the version is *correct* for the current shape. No test can
//! know that a struct change was shape-breaking. What is mechanizable is that the four copies AGREE, so
//! a deliberate move has to be made in all four places at once and an accidental one fails loudly.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = repo().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

#[test]
fn every_shipped_copy_of_the_envelope_contract_version_agrees_with_the_constant() {
    let version = zzop_core::NORMALIZED_AST_CONTRACT_VERSION;

    // 1. The example an adapter author copies verbatim.
    let example = read("docs/contracts/example-envelope.json");
    let declared = serde_json::from_str::<serde_json::Value>(&example)
        .expect("example-envelope.json must parse")
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .expect("example-envelope.json must declare a string `version`");
    assert_eq!(
        declared, version,
        "docs/contracts/example-envelope.json declares version {declared} but \
         NORMALIZED_AST_CONTRACT_VERSION is {version}. Both ship in the binary, so an adapter author \
         copying the example emits a version the engine does not agree is current. If the SHAPE moved, \
         move all four copies; if it did not, neither may move — see this file's module doc."
    );

    // 2/3. The schema and the guide, each of which states the version in prose a guard cannot parse
    // structurally — so assert the literal is present rather than trying to re-derive their sentences.
    for (rel, why) in [
        (
            "docs/adapters/envelope.schema.json",
            "the schema's `version` description says which contract it documents",
        ),
        (
            "docs/NORMALIZED_AST.md",
            "the guide tells the author which value to emit",
        ),
    ] {
        let text = read(rel);
        assert!(
            text.contains(version),
            "{rel} never mentions {version}, but that is the current \
             NORMALIZED_AST_CONTRACT_VERSION — {why}, so it is a second owner of this fact and has \
             drifted."
        );
    }
}

/// The version must not silently become the workspace version. They are equal only by coincidence when
/// a release happens to change the shape; a test that asserted equality would re-introduce exactly the
/// calendar-tracking defect the constant's ruling rejects. So this asserts the opposite direction: the
/// propagation guard must still be excluding the example envelope.
#[test]
fn the_release_propagation_guard_still_excludes_the_example_envelope() {
    let guard = read("scripts/check-release-version-propagation.sh");
    assert!(
        guard.contains("docs/contracts/example-envelope"),
        "scripts/check-release-version-propagation.sh no longer names \
         docs/contracts/example-envelope.json. That guard rewrites every semver-shaped `\"version\"` in \
         tracked JSON to the workspace version, and this one file must be exempt — it carries the \
         envelope SHAPE version, whose contract is that it does not track releases. Without the \
         exclusion the next release silently breaks every 0.x adapter again."
    );
}
