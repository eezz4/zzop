//! Rule-pack definition types — the serde surface that deserializes `rules/dsl/*.json`.
//!
//! This module holds the pack/rule envelope (`RulePackDef`, `RuleDef`). The `${NAME}` fragment
//! expansion applied to that envelope lives in the sibling `expand` submodule, and the defect/opinion
//! axis a rule declares in `axis` — both split off for the per-file line cap. The four
//! matcher shapes (`Matcher` + `LineScan`/`MethodScan`/`SymbolScan`/`IoScan` + `LabeledPattern`/
//! `IoDirection`) live in the sibling `matcher` submodule purely to keep each file under the repo's
//! per-file line cap; they are re-exported below so every external path
//! (`zzop_core::dsl::def::{RulePackDef, Matcher, LineScan, …}`) is unchanged.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::Severity;

mod axis;
mod expand;
mod matcher;
mod pattern_fields;
mod provenance;

pub use axis::RuleAxis;
pub use matcher::{
    CallScan, IoDirection, IoScan, LabeledPattern, LineScan, LiteralScan, Matcher, MethodScan,
    SymbolScan,
};
pub(crate) use pattern_fields::for_each_pattern_field;
pub use provenance::PackExport;

/// A rule pack (DSL) — maps to one `rules/dsl/<id>.json`. Independently shipped and versioned.
///
/// ## The removed `framework` key (2026-08-11)
/// This struct carried a `framework: String` (default `"any"`) until 1.0 preparation. It was deleted
/// rather than frozen, and the reason is the one this repo keeps applying: **it had no reader.** Every
/// shipped pack wrote the same single value `"any"`, no engine code path ever consulted it, and
/// `docs/rules/dsl-reference.md` said so out loud — so it was a documented no-op, not a working knob.
/// 1.0 freezes the third-party pack schema; a key with nothing behind it would have become a promise
/// this engine could not keep until 2.0. Same shape as v0.30.0's `typeSafety`/`lod` deletion, with one
/// asymmetry worth naming: those were OUTPUT fields, so removal could not touch anyone's file, while
/// this is an INPUT key a third party may have written.
///
/// **That asymmetry is why removal is safe here specifically**: this struct does NOT use
/// `#[serde(deny_unknown_fields)]`, so a pack still declaring `"framework": "react"` keeps loading and
/// the key is ignored. `tests::a_pack_still_declaring_the_removed_framework_key_loads` pins exactly
/// that — the compatibility claim is machine-held, so adding `deny_unknown_fields` later cannot
/// silently break every pack written before this deletion.
#[derive(Debug, Clone, Deserialize)]
pub struct RulePackDef {
    pub id: String,
    /// DSL schema version this pack was authored against (see `docs/rules/dsl-reference.md`). Defaults to
    /// `1` when absent, so packs predating this field keep loading. `pack_loader::load_dsl_packs` rejects a
    /// pack whose version exceeds `pack_loader::SUPPORTED_DSL_SCHEMA_VERSION` as a mismatch, not new data to
    /// silently misinterpret; older-or-equal versions always load since schema evolution is additive-only.
    #[serde(default = "current_dsl_schema_version")]
    pub schema_version: u32,
    /// Named regex fragments, referenced from any pattern-bearing field below via a whole-value `${NAME}`
    /// string (see `expand_fragments`'s doc for the full mechanism). Merged UNDER the shared bundled set
    /// (`dsl::fragments::shared_fragments`) at expansion time — a name declared here WINS a collision
    /// against a shared fragment of the same name, so a pack can locally override a shared idiom.
    /// `BTreeMap` (not `HashMap`) so this pack's `Debug`/hash output stays deterministic across runs
    /// (irrelevant post-expansion, since `expand_fragments` clears this field to empty, but relevant for
    /// error-message determinism and for a pack that hasn't been expanded yet, e.g. an inline `packDefs`
    /// entry mid-request). Empty (the default) for a pack that references only shared fragments, or none.
    #[serde(default)]
    pub fragments: BTreeMap<String, String>,
    /// RETRIEVAL stamp — which zzop build served these bytes, and under which contract resource. Present
    /// only on a pack obtained through a shipped binary's contract lane; absent (and silent) on every
    /// hand-written or source-checkout copy. Read by `pack_loader::pack_export_staleness`, which turns a
    /// version difference into a run warning. NOT a second `schema_version`: see [`PackExport`] for why
    /// the format's version and the serving build's version are different facts with different jobs.
    pub exported_from: Option<PackExport>,
    pub rules: Vec<RuleDef>,
    /// PACK-SCOPED compiled-regex memo — never deserialized, never part of a pack's identity. See
    /// [`crate::dsl::RegexCache`] for why the evaluator needs one and why its lifetime is the pack's.
    #[serde(skip)]
    pub regex_cache: crate::dsl::RegexCache,
}

/// Default `RulePackDef::schema_version` for packs predating the field — always `1` (the oldest schema),
/// not `SUPPORTED_DSL_SCHEMA_VERSION`, even after that constant is bumped for a future schema revision.
fn current_dsl_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleDef {
    pub id: String,
    /// Defect or opinion — see [`RuleAxis`] for what the two mean, why `severity` is not this axis,
    /// and why the default is the one it is.
    #[serde(default)]
    pub axis: RuleAxis,
    pub severity: Severity,
    /// Human-facing message (cause / fix hint).
    pub message: String,
    pub matcher: Matcher,
    /// OPT OUT of the test-region gate: `true` means this rule keeps judging lines a parser proved are
    /// compiled out of the shipping build (`SourceFile::test_spans` — see `crate::dsl::eval`'s
    /// `TestRegions`). Default `false` = gated, which is what almost every rule wants.
    ///
    /// ## Why this is a RULE field and not a matcher field
    /// The gate is applied in `eval_pack_impl` AFTER a rule's matcher has run, over the findings it
    /// produced — it never consults the matcher's shape. Putting the opt-out on `LineScan` would leave
    /// `MethodScan`/`SymbolScan`/`CallScan` silently unable to ask for it; putting it on every matcher
    /// struct would be one fact written once per kind. It sits where the gate sits: on the rule.
    ///
    /// ## What justifies setting it
    /// Exactly one axis: the finding is about a **credential at rest**, where the COMMIT is the leak and
    /// the code's execution status is irrelevant to the verdict. A PEM header or a `user:pass@host` URL
    /// inside `#[cfg(test)] mod tests` is still a key in git history, readable by every fork and clone,
    /// and still has to be rotated. Every other rule class judges code that RUNS, and test-only code
    /// does not run in production — for those the gate is right and this flag must stay `false`.
    ///
    /// The same axis is already visible in the packs as the ABSENCE of
    /// `file_exclude_pattern: "${test-paths-stories}"` on those rules: they decline PATH-based test
    /// exclusion for this reason, so declining SPAN-based exclusion is the same decision with the other
    /// kind of evidence. A rule that sets this while still excluding test PATHS would be incoherent,
    /// and `crates/facade/src/test_region_promise_tests.rs` refuses that combination — along with any
    /// drift between this flag and the promise the catalog/message publishes to users.
    #[serde(default)]
    pub scan_test_regions: bool,
}

impl RuleDef {
    /// Inline ok-marker for this rule, DERIVED as `zzop-<id>-ok` (never stored on the rule) — e.g. rule
    /// `float-money-compare` suppresses on `// zzop-float-money-compare-ok`. Applied uniformly to
    /// `LineScan` and `MethodScan` findings: a finding is suppressed when its own line, or the line
    /// directly above it (`MARKER_LOOKBACK_LINES`), carries a `//`-comment naming this marker
    /// (`// zzop-<id>-ok` or `// zzop-<id>-ok: reason` both suppress). For a file whose extension is
    /// `.sql` (case-insensitive, see `markers::leaders_for_path`), a `--`-comment naming the marker suppresses
    /// identically (`-- zzop-<id>-ok`) — `--` is a line comment in SQL but not in JS/TS (`--x` is a
    /// decrement there), so that recognition is gated to `.sql` files only. Deriving (vs storing a
    /// per-rule string) means the marker is always predictable from the id and can never drift out of the
    /// convention.
    ///
    /// The `zzop-` TOOL PREFIX is deliberate and matches what every neighbouring tool does — ESLint's
    /// `eslint-disable-*`, TypeScript's `@ts-ignore`. Without it a suppression comment could not be
    /// grepped as a class, and a reader finding one in a diff could not tell WHOSE checker it silenced.
    /// The `-ok` suffix is kept (rather than `-skip`/`-ignore`) because it asserts something stronger and
    /// more specific: a human looked at this finding and vetted it, which `eslint-disable` does not claim.
    pub fn suppress_marker(&self) -> String {
        Self::suppress_marker_for_id(&self.id)
    }

    /// The same derivation reached by id alone, for callers that must spell the marker form without a
    /// loaded rule — notably the MCP `rule-catalog` resource description, which ships the spelling to
    /// agent clients that have no source checkout. Exists so those surfaces can PIN against the
    /// derivation instead of hardcoding a copy of it: the 2026-07-26 prefix change migrated every doc
    /// but left one shipped description advertising the retired bare form.
    pub fn suppress_marker_for_id(id: &str) -> String {
        format!("zzop-{id}-ok")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compatibility half of the `framework` deletion (this module's doc). A pack written before
    /// 2026-08-11 may still declare the key, and it must keep loading — the deletion removes a no-op,
    /// it does not invalidate anyone's file. This test is what makes that claim survive: adding
    /// `#[serde(deny_unknown_fields)]` to `RulePackDef` later would break every such pack silently,
    /// and it turns this red instead (verified by planting exactly that attribute).
    #[test]
    fn a_pack_still_declaring_the_removed_framework_key_loads() {
        let json = r#"{
            "id": "legacy",
            "framework": "react",
            "schema_version": 1,
            "rules": []
        }"#;
        let pack: RulePackDef =
            serde_json::from_str(json).expect("a pre-deletion pack must still load");
        assert_eq!(pack.id, "legacy");
        assert_eq!(pack.schema_version, 1);
        assert!(pack.rules.is_empty());
    }

    /// The other direction: a pack that never declared it loads too, which is what every shipped pack
    /// now looks like. Without this the test above would also pass on a struct that had gained a
    /// REQUIRED `framework` field.
    #[test]
    fn a_pack_omitting_it_entirely_loads() {
        let pack: RulePackDef =
            serde_json::from_str(r#"{ "id": "modern", "rules": [] }"#).expect("must load");
        assert_eq!(pack.id, "modern");
        assert_eq!(pack.schema_version, current_dsl_schema_version());
    }
}
