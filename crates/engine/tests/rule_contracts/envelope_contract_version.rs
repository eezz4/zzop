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
//! ## The other axis, and why every committed ENVELOPE lives on this one
//! `scripts/check-release-version-propagation.sh` owns the RELEASE axis: every committed `"version"` on
//! the release surface equals `Cargo.toml`'s `[workspace.package] version`. An envelope's `"version"` is
//! not on that surface — it names the SHAPE the bytes conform to. The two numbers are equal today only
//! by coincidence, and the moment the contract legitimately lags the release (its normal state), the
//! release guard would force every committed envelope to declare a contract version that does not exist.
//! The guard therefore classifies envelopes by their own bytes (`"format": "zzop-normalized-ast"`) and
//! holds them out; [`every_committed_envelope_declares_a_contract_version_this_build_accepts`] is where
//! they land instead, so neither axis has an unguarded hole.
//!
//! What that test can bind is NOT equality with the constant. An envelope declaring an older contract
//! version is the contract WORKING — `examples/adapters/*` deliberately still emit `0.27.0` to prove a
//! producer survives releases it predates — so demanding equality would be the calendar defect one level
//! out. What holds for every envelope regardless is that its declared version is a real contract version
//! this build accepts: parseable, at or below the current contract, and passing `validate_envelope`.
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

/// A per-feature floor (`MIN_VERSION_FOR_*`) pins to the release that introduced ITS feature, and a
/// later contract bump must not drag it along — a floor that tracks the contract version answers "what
/// is current" to a reader who asked "since when". No test can see that a floor was wrongly moved, so
/// this asserts the two things that hold either way: a floor is a parseable version at or below the
/// current contract version (above it would name a release no conforming producer may declare), and the
/// guide states its value, since a floor no author can read rejects envelopes for an invisible reason.
#[test]
fn every_per_feature_floor_is_stated_in_the_guide_and_at_or_below_the_contract_version() {
    let contract = zzop_core::parse_contract_version(zzop_core::NORMALIZED_AST_CONTRACT_VERSION)
        .expect("NORMALIZED_AST_CONTRACT_VERSION must be MAJOR.MINOR.PATCH");
    let guide = read("docs/NORMALIZED_AST.md");
    for (name, floor) in [
        (
            "MIN_VERSION_FOR_OVERRIDES",
            zzop_core::MIN_VERSION_FOR_OVERRIDES,
        ),
        (
            "MIN_VERSION_FOR_ROUTER_MOUNT_REF",
            zzop_core::MIN_VERSION_FOR_ROUTER_MOUNT_REF,
        ),
    ] {
        let parsed = zzop_core::parse_contract_version(floor)
            .unwrap_or_else(|| panic!("{name} ({floor}) must be MAJOR.MINOR.PATCH"));
        assert!(
            parsed <= contract,
            "{name} is {floor}, above the current contract version {}. A floor names the release that \
             introduced its feature, so it can never exceed the release whose shape is current — one of \
             the two moved wrongly.",
            zzop_core::NORMALIZED_AST_CONTRACT_VERSION
        );
        assert!(
            guide.contains(floor),
            "docs/NORMALIZED_AST.md never mentions {floor}, the value of {name}. An adapter author is \
             told which version to declare by that document alone; a floor it does not state rejects \
             envelopes for a reason their author cannot read."
        );
    }
}

/// Every tracked file whose own bytes say it is a Normalized-AST envelope, as `(repo-relative path,
/// declared version)`. Classified by CONTENT — top-level `"format": "zzop-normalized-ast"` — never by
/// path or extension, so an envelope added anywhere in the tree joins this subject set without an edit
/// here. A path list is the second copy of a fact this repo keeps paying for, and this exact subject had
/// one: the guard named `docs/contracts/example-envelope.json` by hand while `cases/`'s two envelopes
/// sat on the release axis, two envelopes under two rules with nothing saying which was right.
///
/// The subject is derived from `git ls-files` for the same reason `markers.rs` derives its pages there:
/// a hardcoded root can stop resolving and a green result then means nothing was read.
fn committed_envelopes() -> Vec<(String, String)> {
    let root = repo();
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "-z"])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "could not run `git ls-files` in {} ({e}) — this contract DERIVES its envelopes from \
                 git, so without it there is no subject set and a green result would mean nothing was \
                 read",
                root.display()
            )
        });
    assert!(
        out.status.success(),
        "`git ls-files` failed in {}: {}",
        root.display(),
        String::from_utf8_lossy(&out.stderr)
    );

    let mut found = Vec::new();
    for rel in String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
    {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue; // binary or deleted-in-worktree; neither can be an envelope
        };
        // Cheap prefilter, then the real test: it must PARSE as JSON and carry the format string as a
        // top-level field. Without the parse, this crate's own test fixtures — which embed envelope
        // text inside Rust string literals — would be classified as envelopes.
        if !text.contains(zzop_core::NORMALIZED_AST_FORMAT) {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if value.get("format").and_then(|v| v.as_str()) != Some(zzop_core::NORMALIZED_AST_FORMAT) {
            continue;
        }
        let declared = value
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                panic!("{rel} is a Normalized-AST envelope but declares no string `version`")
            });
        found.push((rel.to_string(), declared.to_string()));
    }
    found
}

/// The whole committed envelope population, on the contract axis rather than the release one.
///
/// Equality with the constant is deliberately NOT the assertion — see the module doc. What is asserted
/// is that each declared version is a contract version that exists and that this build accepts, which is
/// exactly the property the release guard would have destroyed by dragging these files onto the calendar.
#[test]
fn every_committed_envelope_declares_a_contract_version_this_build_accepts() {
    let contract = zzop_core::parse_contract_version(zzop_core::NORMALIZED_AST_CONTRACT_VERSION)
        .expect("NORMALIZED_AST_CONTRACT_VERSION must be MAJOR.MINOR.PATCH");
    let envelopes = committed_envelopes();

    // Non-empty floor, mirroring the one the release guard keeps on its own extraction: a classifier
    // that stopped matching would report a clean tree having read nothing, and the envelopes would
    // silently fall back onto the release axis with no test left holding them.
    assert!(
        !envelopes.is_empty(),
        "classified ZERO committed Normalized-AST envelopes. The `\"format\": \
         \"{}\"` self-identification was reshaped, or `git ls-files` returned nothing — either way \
         this contract read no bytes and its green means nothing.",
        zzop_core::NORMALIZED_AST_FORMAT
    );
    assert!(
        envelopes
            .iter()
            .any(|(rel, _)| rel == "docs/contracts/example-envelope.json"),
        "the envelope an adapter author copies (docs/contracts/example-envelope.json) is not in the \
         classified set {envelopes:?} — the content classification is no longer finding the file this \
         contract is most about."
    );

    for (rel, declared) in &envelopes {
        let parsed = zzop_core::parse_contract_version(declared).unwrap_or_else(|| {
            panic!(
                "{rel} declares version {declared}, which is not MAJOR.MINOR.PATCH. Every envelope \
                 version names a RELEASE whose shape it conforms to, and a string no consumer can \
                 order against is rejected by every engine that reads it."
            )
        });
        assert!(
            parsed <= contract,
            "{rel} declares envelope version {declared}, ABOVE the current contract version {}. That \
             names a shape no build in this repo produces or accepts (\"reject newer, never guess\"). \
             Declaring an OLDER version is fine and is the contract working; declaring a newer one \
             means either the shape moved without the constant, or this file was dragged onto the \
             RELEASE version by something that mistook the two axes for one — see this file's \
             module doc.",
            zzop_core::NORMALIZED_AST_CONTRACT_VERSION
        );
        if let Err(issues) = zzop_core::validate_envelope(&read(rel)) {
            panic!(
                "{rel} is a committed envelope this build REJECTS: {issues:?}. A shipped envelope that \
                 does not validate is one an adapter author may copy — including the per-feature floors \
                 (`MIN_VERSION_FOR_*`), which a declared version moved by the wrong axis violates."
            );
        }
    }
}

/// The release axis must keep classifying envelopes by CONTENT, not by a path list. This is the seam
/// between the two guards, and it is the seam that was wrong: the shell guard named one envelope by path
/// and silently held the others to the release version.
#[test]
fn the_release_propagation_guard_classifies_envelopes_by_content() {
    let guard = read("scripts/check-release-version-propagation.sh");
    assert!(
        guard.contains(zzop_core::NORMALIZED_AST_FORMAT),
        "scripts/check-release-version-propagation.sh no longer mentions \"{}\", so it is no longer \
         classifying envelopes by their own bytes. That guard forces every semver-shaped `\"version\"` \
         in tracked JSON to the workspace version, and an envelope's version is the SHAPE contract, \
         which does not track releases. Without the content test the next release silently breaks \
         every 0.x adapter again.",
        zzop_core::NORMALIZED_AST_FORMAT
    );
    assert!(
        !guard.contains("^docs/contracts/example-envelope"),
        "scripts/check-release-version-propagation.sh has an anchored PATH exclusion for the example \
         envelope again. Content classification already covers it; two mechanisms for one exemption is \
         how the other envelopes ended up on the wrong axis in the first place."
    );
}
