//! Rule-pack definition types — the serde surface that deserializes `rules/dsl/*.json`.
//!
//! This module holds the pack/rule envelope (`RulePackDef`, `RuleDef`) plus the `${NAME}` fragment
//! expansion logic (`RulePackDef::expand_fragments` and its private `resolve_*` helpers). The four
//! matcher shapes (`Matcher` + `LineScan`/`MethodScan`/`SymbolScan`/`IoScan` + `LabeledPattern`/
//! `IoDirection`) live in the sibling `matcher` submodule purely to keep each file under the repo's
//! per-file line cap; they are re-exported below so every external path
//! (`zzop_core::dsl::def::{RulePackDef, Matcher, LineScan, …}`) is unchanged.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::fragments::{fragment_ref_name, shared_fragments, FragmentError};
use crate::Severity;

mod matcher;
mod pattern_fields;

pub use matcher::{
    CallScan, IoDirection, IoScan, LabeledPattern, LineScan, LiteralScan, Matcher, MethodScan,
    SymbolScan,
};
pub(crate) use pattern_fields::for_each_pattern_field;

/// A rule pack (DSL) — maps to one `rules/dsl/<id>.json`. Independently shipped and versioned.
#[derive(Debug, Clone, Deserialize)]
pub struct RulePackDef {
    pub id: String,
    #[serde(default = "any_framework")]
    pub framework: String,
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
    pub rules: Vec<RuleDef>,
    /// PACK-SCOPED compiled-regex memo — never deserialized, never part of a pack's identity. See
    /// [`crate::dsl::RegexCache`] for why the evaluator needs one and why its lifetime is the pack's.
    #[serde(skip)]
    pub regex_cache: crate::dsl::RegexCache,
}

fn any_framework() -> String {
    "any".into()
}

/// Default `RulePackDef::schema_version` for packs predating the field — always `1` (the oldest schema),
/// not `SUPPORTED_DSL_SCHEMA_VERSION`, even after that constant is bumped for a future schema revision.
fn current_dsl_schema_version() -> u32 {
    1
}

/// Resolves a single pattern-bearing `String` field in place, if (and only if) its ENTIRE value is a
/// `${NAME}` fragment reference (see `fragments::fragment_ref_name`'s doc for why this whole-value-only
/// shape is collision-safe). A value that merely CONTAINS `${...}` as a substring is left untouched — no
/// inline substring composition in this pass.
fn resolve_field(
    value: &mut String,
    merged: &BTreeMap<String, String>,
    rule_id: &str,
    field: &str,
) -> Result<(), FragmentError> {
    let Some(name) = fragment_ref_name(value) else {
        return Ok(());
    };
    let Some(text) = merged.get(name) else {
        return Err(FragmentError::Unknown {
            rule: rule_id.to_string(),
            field: field.to_string(),
            name: name.to_string(),
        });
    };
    if fragment_ref_name(text).is_some() {
        return Err(FragmentError::Nested {
            rule: rule_id.to_string(),
            field: field.to_string(),
            name: name.to_string(),
        });
    }
    *value = text.clone();
    Ok(())
}

impl RulePackDef {
    /// Resolves every whole-value `${NAME}` fragment reference across every pattern-bearing field in this
    /// pack — the field set is not restated here, it is walked by
    /// [`pattern_fields::for_each_pattern_field`], whose exhaustive destructuring is what makes "every"
    /// true (see that module's header) — then CLEARS `self.fragments` to empty — so a
    /// pack that never referenced a fragment at all, and a pack that resolved every `${NAME}` ref, end up
    /// `Debug`/hash-identical to each other (and to the equivalent pack authored with the patterns spelled
    /// out inline). This is what makes the migration in this pass projection-neutral: `{pack:?}` — the
    /// cache fingerprint input (`crates/engine/src/cache.rs`) — is byte-for-byte unchanged for every pack
    /// this expansion touches, so no cache-schema/interpreter-fingerprint bump rides with it.
    /// (Byte-identity is intra-version: adding the `fragments` field itself makes the derived `Debug`
    /// emit `fragments: {}`, which a PRIOR release without the field did not — so upgrading across this
    /// change is a one-time, harmless cache cold-start for every pack, recomputing to identical findings.
    /// That is a field-addition effect, not a migration effect, and needs no bump for correctness.)
    ///
    /// Reference names resolve against `self.fragments` merged UNDER the shared bundled set
    /// (`dsl::fragments::shared_fragments`) — a per-pack name wins a collision against a shared one of the
    /// same name.
    ///
    /// This is a SINGLE pass, deliberately not recursive: a fragment's own resolved text is never itself
    /// re-scanned for further `${NAME}` refs. A fragment whose value is itself a whole-value `${...}`
    /// reference is a hard [`FragmentError::Nested`], not a silently-inert passthrough or a chained
    /// expansion — same "fail the load, don't guess" contract an unknown name gets
    /// ([`FragmentError::Unknown`]). Call this at every `RulePackDef` deserialize boundary BEFORE the pack
    /// is hashed or evaluated: `pack_loader::parse_dsl_pack` (disk load, the `validate_rule_pack`
    /// validator, and bundled-pack parsing all funnel through it) and the inline `packDefs` wire path
    /// (`zzop-facade`'s `base_engine_config`, which owns every `RulePackDef` deserialized directly off an
    /// `AnalyzeRequest`/`EnvelopeAnalyzeRequest` — a boundary `parse_dsl_pack` never sees, since serde
    /// deserializes those `Vec<RulePackDef>` fields directly, not through pack JSON *text*).
    ///
    /// Idempotent: calling this again on an already-expanded pack (`fragments` empty, no `${NAME}` values
    /// remaining) is a no-op — safe for a pack that reaches this call twice across two merged sources
    /// (e.g. a bundled pack, already expanded via `parse_dsl_pack`, folded into the same `pack_defs` list
    /// `base_engine_config` re-expands every entry of).
    pub fn expand_fragments(&mut self) -> Result<(), FragmentError> {
        let merged: BTreeMap<String, String> = if self.fragments.is_empty() {
            shared_fragments().clone()
        } else {
            let mut merged = shared_fragments().clone();
            merged.extend(self.fragments.iter().map(|(k, v)| (k.clone(), v.clone())));
            merged
        };

        for rule in &mut self.rules {
            let rid = rule.id.clone();
            for_each_pattern_field(rule, &mut |field, value| {
                resolve_field(value, &merged, &rid, field)
            })?;
        }

        self.fragments.clear();
        Ok(())
    }

    /// ADDS `extra` (one already-validated regex source) to every rule in this pack whose
    /// `file_exclude_pattern` is the shared `${test-paths…}` vocabulary, rewriting it to
    /// `(?:<shared>)|(?:<extra>)`. Returns how many rules were rewritten, so a caller can self-report a
    /// declaration that moved nothing. Call it AFTER [`expand_fragments`] — before expansion the value
    /// is still a `${NAME}` reference and nothing matches.
    ///
    /// ## Why this is ADDITIVE, when every other declared vocabulary REPLACES
    /// `zzop_engine::VocabularyConfig`'s standing contract is per-key whole replacement, and its default
    /// for an absent key is "the judgment is NOT MADE" — an under-report the caller chose. Test paths
    /// invert both halves, and the reason is the direction of the failure, not a preference:
    /// * A test convention is fixed by the LANGUAGE, not chosen by the project. `_test.go` is what the
    ///   Go toolchain compiles as a test; asking a Go user to declare it is asking them to restate the
    ///   toolchain, and it breaks zero-config for every language whose convention we already know.
    /// * With no default, an undeclared vocabulary means the engine judges test code as production.
    ///   That is a WRONG CLAIM, not an abstention — the opposite failure direction from every other key
    ///   here, where silence merely means we say less. Different direction, different policy.
    /// * Replacement would be a trap: a project adding `it/` for its integration tests would silently
    ///   lose `_test.go`, `test_*.py` and `*Tests.cs` in the same edit, and the loss would show up as
    ///   findings rather than as an error.
    ///
    /// So the built-in conventions are floor, never ceiling, and a declaration can only widen what is
    /// declined. Narrowing is deliberately not expressible here: `rules: { "<id>": "off" }` turns a rule
    /// off, and that is the knob for "I want to be judged on this path after all".
    pub fn extend_test_path_exclusions(&mut self, extra: &str) -> usize {
        let mut extended = 0usize;
        for rule in &mut self.rules {
            let _ =
                for_each_pattern_field::<std::convert::Infallible>(rule, &mut |field, value| {
                    if field == "file_exclude_pattern"
                        && crate::dsl::fragments::is_shared_test_path_vocabulary(value)
                    {
                        *value = format!("(?:{value})|(?:{extra})");
                        extended += 1;
                    }
                    Ok(())
                });
        }
        extended
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleDef {
    pub id: String,
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
