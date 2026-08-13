//! `${NAME}` FRAGMENT EXPANSION — [`RulePackDef::expand_fragments`] and its one field-level helper.
//!
//! Its own file since 2026-08-12, when the `axis` field pushed the parent `def/mod.rs` past the repo
//! line cap. The cut is along a seam the parent module doc already named: `mod.rs` carries the pack/rule
//! SHAPE (what a pack file deserializes into) and this carries the one TRANSFORM applied to that shape
//! before it is hashed or evaluated. Nothing here is new — the code, its contracts and its reasoning
//! moved verbatim.

use std::collections::BTreeMap;

use super::{for_each_pattern_field, RulePackDef};
use crate::dsl::fragments::{fragment_ref_name, shared_fragments, FragmentError};

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
