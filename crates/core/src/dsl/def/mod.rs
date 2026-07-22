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

pub use matcher::{IoDirection, IoScan, LabeledPattern, LineScan, Matcher, MethodScan, SymbolScan};

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

/// Same as `resolve_field`, for an `Option<String>` field — a `None` field has nothing to resolve.
fn resolve_opt(
    value: &mut Option<String>,
    merged: &BTreeMap<String, String>,
    rule_id: &str,
    field: &str,
) -> Result<(), FragmentError> {
    match value {
        Some(v) => resolve_field(v, merged, rule_id, field),
        None => Ok(()),
    }
}

/// Same as `resolve_field`, applied to every element of a `Vec<String>` field (`require_file_all`/
/// `require_file_absent`) — each element is independently eligible for a whole-value `${NAME}` ref.
fn resolve_vec(
    values: &mut [String],
    merged: &BTreeMap<String, String>,
    rule_id: &str,
    field: &str,
) -> Result<(), FragmentError> {
    for v in values.iter_mut() {
        resolve_field(v, merged, rule_id, field)?;
    }
    Ok(())
}

/// Same as `resolve_field`, applied to every `LabeledPattern::pattern` in a slice (`any`/`patterns`/
/// `absent`) — the `label` alongside it is never pattern-bearing, so it is never a fragment-ref target.
fn resolve_labeled(
    patterns: &mut [LabeledPattern],
    merged: &BTreeMap<String, String>,
    rule_id: &str,
    field: &str,
) -> Result<(), FragmentError> {
    for lp in patterns.iter_mut() {
        resolve_field(&mut lp.pattern, merged, rule_id, field)?;
    }
    Ok(())
}

impl RulePackDef {
    /// Resolves every whole-value `${NAME}` fragment reference across every pattern-bearing field in this
    /// pack (`file_pattern`, `file_exclude_pattern`, `require_file`, `require_file_all`,
    /// `require_file_absent`, `line_pattern`, `any[].pattern`, `exclude_pattern`, `patterns[].pattern`,
    /// `absent[].pattern`, `name_pattern`, `key_pattern`), then CLEARS `self.fragments` to empty — so a
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
            match &mut rule.matcher {
                Matcher::LineScan(m) => {
                    resolve_field(&mut m.file_pattern, &merged, &rid, "file_pattern")?;
                    resolve_opt(&mut m.require_file, &merged, &rid, "require_file")?;
                    resolve_vec(&mut m.require_file_all, &merged, &rid, "require_file_all")?;
                    resolve_vec(
                        &mut m.require_file_absent,
                        &merged,
                        &rid,
                        "require_file_absent",
                    )?;
                    resolve_opt(&mut m.line_pattern, &merged, &rid, "line_pattern")?;
                    if let Some(any) = m.any.as_mut() {
                        resolve_labeled(any, &merged, &rid, "any[].pattern")?;
                    }
                    resolve_opt(&mut m.exclude_pattern, &merged, &rid, "exclude_pattern")?;
                    resolve_opt(
                        &mut m.file_exclude_pattern,
                        &merged,
                        &rid,
                        "file_exclude_pattern",
                    )?;
                }
                Matcher::MethodScan(m) => {
                    resolve_field(&mut m.file_pattern, &merged, &rid, "file_pattern")?;
                    resolve_opt(&mut m.require_file, &merged, &rid, "require_file")?;
                    resolve_vec(&mut m.require_file_all, &merged, &rid, "require_file_all")?;
                    resolve_vec(
                        &mut m.require_file_absent,
                        &merged,
                        &rid,
                        "require_file_absent",
                    )?;
                    resolve_labeled(&mut m.patterns, &merged, &rid, "patterns[].pattern")?;
                    resolve_labeled(&mut m.absent, &merged, &rid, "absent[].pattern")?;
                    resolve_opt(
                        &mut m.file_exclude_pattern,
                        &merged,
                        &rid,
                        "file_exclude_pattern",
                    )?;
                }
                Matcher::SymbolScan(m) => {
                    resolve_field(&mut m.file_pattern, &merged, &rid, "file_pattern")?;
                    resolve_opt(&mut m.name_pattern, &merged, &rid, "name_pattern")?;
                }
                Matcher::IoScan(m) => {
                    resolve_field(&mut m.file_pattern, &merged, &rid, "file_pattern")?;
                    resolve_opt(
                        &mut m.file_exclude_pattern,
                        &merged,
                        &rid,
                        "file_exclude_pattern",
                    )?;
                    resolve_opt(&mut m.key_pattern, &merged, &rid, "key_pattern")?;
                }
            }
        }

        self.fragments.clear();
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleDef {
    pub id: String,
    pub severity: Severity,
    /// Human-facing message (cause / fix hint).
    pub message: String,
    pub matcher: Matcher,
    /// Inline ok-marker suppression, applied uniformly to `LineScan` and `MethodScan` findings. A finding
    /// is suppressed when its own line, or the line directly above it (`MARKER_LOOKBACK_LINES`), contains a
    /// `//`-comment naming this marker (`// n+1-ok` or `// n+1-ok: reason` both suppress `suppress_marker:
    /// "n+1-ok"`). For a file whose extension is `.sql` (case-insensitive, see `is_sql_file`), a `--`-comment
    /// naming the marker suppresses identically (`-- n+1-ok`) — `--` is a line comment in SQL but not in
    /// JS/TS (`--x` is a decrement there), so this recognition is gated to `.sql` files only and never
    /// changes behavior for any other extension.
    #[serde(default)]
    pub suppress_marker: Option<String>,
}
