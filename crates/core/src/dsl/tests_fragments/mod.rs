//! Tests for the `${NAME}` DSL rule-pack fragment mechanism (`def::RulePackDef::expand_fragments`,
//! `fragments::shared_fragments`/`fragment_ref_name`). Split into three submodules purely to stay under
//! the repo's per-file line cap:
//! - `expansion_tests` — synthetic-pack unit coverage of the resolution/error contract (`fragment_ref_name`,
//!   shared-vs-per-pack precedence, unknown/nested errors, idempotency, every-field coverage);
//! - `byte_identity` — the real-`rules/dsl`-tree guards: the sentinel-collision check every shipped pack
//!   must pass, that the tree loads with zero errors, and the `Debug`-unchanged byte-identity proof for
//!   two non-`sql` migrated packs.
//! - `superset` — the one relationship the mechanism itself cannot express: a fragment that EXTENDS
//!   another must still CONTAIN all of it (see that module's doc for why a reference can't say it).
//! - `name_census` — the triage moment for a NEW fragment name, moved here from
//!   `scripts/check-policy-census.sh` (see that module's doc for the measured reason a shell
//!   extractor could not hold this axis).

use std::path::{Path, PathBuf};

use super::def::RulePackDef;

mod byte_identity;
mod expansion_tests;
mod name_census;
mod superset;

/// The real, committed pack tree — never a synthetic fixture, so every guard in this module reads what
/// actually ships.
pub(super) fn real_dsl_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl")
}

/// `path` rendered relative to the repo root (`rules/dsl/<...>`, forward slashes on every platform), so
/// a failure message and a censused row name a file the way the repo does — not as this machine's
/// absolute `.../crates/core/../../rules/dsl/...` path.
pub(super) fn repo_rel(path: &Path) -> String {
    let rel = path.strip_prefix(real_dsl_dir()).unwrap_or(path);
    format!("rules/dsl/{}", rel.to_string_lossy().replace('\\', "/"))
}

/// Every shipped pack parsed RAW — a plain `serde_json::from_str`, deliberately NOT `parse_dsl_pack`, so
/// `fragments` is still populated and any `${NAME}` sentinel is still visible (`expand_fragments` resolves
/// the sentinels and then CLEARS the map). Each pack is paired with its path so a failure message names
/// the offending file. One definition of "where the packs are" for every guard in this module.
///
/// Scope is the LOADER CONTRACT — flat (`rules/dsl/<id>.json`) plus depth-1 nested
/// (`rules/dsl/<name>/<id>.json`), the two shapes `pack_loader::load_dsl_packs` documents and reads.
/// That is complete only because the committed tree is held to those two shapes by
/// `the_committed_pack_tree_stays_within_the_depth_1_loader_contract` below; without that pin this
/// enumeration would be narrower than the shipping embed (`crates/config/build.rs::collect` recurses to
/// any depth), and a deeper pack would ship while every guard in this module stayed blind to it —
/// measured, see the pin's doc.
pub(super) fn raw_packs() -> Vec<(PathBuf, RulePackDef)> {
    let mut out = Vec::new();

    for entry in std::fs::read_dir(real_dsl_dir()).expect("rules/dsl must exist") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let json_paths: Vec<PathBuf> = if path.is_dir() {
            std::fs::read_dir(&path)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .collect()
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            vec![path]
        } else {
            vec![]
        };

        for json_path in json_paths {
            let text = std::fs::read_to_string(&json_path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", json_path.display()));
            let pack: RulePackDef = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("failed to parse {}: {e}", json_path.display()));
            out.push((json_path, pack));
        }
    }

    // `read_dir` order is filesystem-dependent; sort so a failure message is reproducible.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Every `*.json` under `rules/dsl`, at ANY depth, repo-relative — the enumeration
/// `crates/config/build.rs::collect` performs when it decides what to embed. Deliberately NOT derived
/// from `raw_packs`: the pin below exists to compare the two, so sharing a walk would make it vacuous.
fn every_json_at_any_depth(dir: &Path, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            every_json_at_any_depth(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(repo_rel(&path));
        }
    }
}

/// Policy pin: the committed `rules/dsl` tree holds pack files ONLY where the loader contract can see
/// them — flat (`rules/dsl/<id>.json`) or depth-1 nested (`rules/dsl/<name>/<id>.json`), the two shapes
/// `pack_loader::load_dsl_packs` documents and `docs/rules/authoring-guide.md` publishes.
///
/// ## Why the tree is pinned instead of teaching every reader to recurse
/// FOUR sites answer "where are the packs", and they did not agree. Measured on this tree by dropping
/// one probe pack at depth 1 (`rules/dsl/probe-depth1.json`) and one at depth 3
/// (`rules/dsl/probe_a/probe_b/probe-depth3.json`) and running each enumeration:
///
/// | site | depth 1 | depth 3 |
/// |---|---|---|
/// | `crates/config/build.rs::collect` (the shipping embed, recursive) | sees it | sees it |
/// | `pack_loader::load_dsl_packs` (the documented loader contract) | sees it | blind |
/// | `raw_packs` above (every guard in this module) | sees it | blind |
/// | `scripts/check-policy-census.sh`'s old `rules/dsl/*/*.json` glob | blind | blind |
///
/// So a depth-3 pack SHIPPED (embedded into the binary) while the sentinel-collision check, the
/// zero-error load pin, the superset pin and the census all saw nothing — no `added:`, no `removed:`,
/// complete silence. Widening the three narrow readers would not have closed it either: `load_dsl_packs`
/// is deliberately shallow (see its doc) and that shallowness is PUBLISHED as the packsDir contract, so
/// widening it is a public behavior change, and a depth-3 first-party pack would still behave
/// differently embedded vs. loaded from disk. Pinning the tree to the published contract makes the three
/// readers complete by construction and costs nothing today — every committed pack is already depth-1
/// nested, and the count belongs to the walk in the test below rather than to this prose. The shipping
/// embed keeps its recursion, it simply can no longer reach anything the contract cannot express.
///
/// The flat half of the contract is legal but effectively unused here, and one more reader is tighter
/// still: `scripts/check-deploy-facts-prose.sh` aborts when two pack JSONs share a directory, so a
/// second FLAT pack would red that guard. It aborts LOUDLY, which is why this pin does not narrow to
/// "nested only" on its behalf — but do not read this pin as a promise that a flat second pack is a
/// no-op.
#[test]
fn the_committed_pack_tree_stays_within_the_depth_1_loader_contract() {
    let mut all = Vec::new();
    every_json_at_any_depth(&real_dsl_dir(), &mut all);
    all.sort();
    assert!(
        !all.is_empty(),
        "no *.json found under rules/dsl — this pin would pass vacuously"
    );

    // "rules/dsl/" + at most one directory component before the file name.
    let too_deep: Vec<&String> = all
        .iter()
        .filter(|p| p.trim_start_matches("rules/dsl/").matches('/').count() > 1)
        .collect();
    assert!(
        too_deep.is_empty(),
        "pack file(s) below the depth-1 loader contract: {too_deep:?}. They WOULD be embedded by \
         crates/config/build.rs (it recurses to any depth) and would therefore ship, but \
         pack_loader::load_dsl_packs, raw_packs (and with it every guard in this module) and \
         scripts/check-policy-census.sh all stop at depth 1 — the pack would ship unguarded and \
         unloadable from disk. Move it to rules/dsl/<name>/<id>.json, or widen the published \
         packsDir contract first (crates/core/src/pack_loader.rs + docs/rules/authoring-guide.md) \
         and then widen the readers with it."
    );

    let contract_visible: Vec<String> = raw_packs()
        .into_iter()
        .map(|(path, _)| repo_rel(&path))
        .collect();
    assert_eq!(
        contract_visible, all,
        "the depth-1 enumeration and a full recursive walk of rules/dsl disagree — the two must be \
         the same set for `raw_packs` to be complete"
    );
}
