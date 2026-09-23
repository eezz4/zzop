//! The RULESET half of a per-file cache key: which rule logic produced the cached findings.
//!
//! Split out of `cache.rs` at the 300-line cap (2026-09-06). The seam is the key's own composition —
//! the parser/engine half lives next door in `parser_fingerprint`, the declared-convention half in
//! `vocabulary_fingerprint`, and this file answers only "would different rule logic have produced a
//! different finding for this file?".

use zzop_cache::AnalysisCache;
use zzop_core::RulePackDef;

use super::{EngineConfig, FP_DSL, FP_SCHEMA_RULES};
/// `ruleset_fingerprint`'s native-rule-logic-version token for `pipeline::schema_findings`
/// (`zzop_rules_schema::apply_schema_rules`, wired into the fused per-file pass for Prisma files). Unlike
/// a DSL pack (whose content already changes the fingerprint via `pack:?`), this is Rust logic with no
/// pack content to hash, so the whole crate is hashed instead: `FP_SCHEMA_RULES` (`build.rs`) covers rule
/// bodies, thresholds, and the message/disable-hint text in `rules-schema`'s `message.rs` — everything
/// that reaches a cached finding, not just its output SHAPE.
///
/// A hand-stamped `STRUCTURAL_RULES_VERSION` used to be concatenated in front of that hash. It was
/// removed 2026-09-06 as duplication with no reader, and the way it was duplicated is the lesson: the
/// const lived in `structural.rs`, INSIDE the very closure `FP_SCHEMA_RULES` hashes, so bumping it moved
/// the derived hash anyway. Found reading `"0.33.0"` against a `0.34.0` workspace — its own doc had
/// already conceded, five weeks earlier, that it "no longer has to be right", and the human-readable
/// purpose it claimed to keep ("a version a person can read in a cache path") was never real: the string is fed
/// straight into `ruleset_fingerprint`'s `content_hash` and reaches no path, log, or reply. A value that
/// need not be right, and that nobody can read, has no job left.
fn schema_structural_fingerprint() -> String {
    format!("schema-structural-{FP_SCHEMA_RULES}")
}

/// Logic-version token for the DSL *interpreter* (`zzop_core::dsl`) itself — the same stale-cache gap
/// `schema_structural_fingerprint` closes for native rule logic. Pack JSON already self-invalidates via
/// `{pack:?}` above, but a pure-Rust interpreter semantics change (matcher evaluation, suppress-marker
/// window, ...) alters findings for byte-identical source AND identical pack content — invisible to the
/// key without this token. **Nothing to restamp by hand**: `FP_DSL` is a derived source hash
/// (`crates/engine/build.rs`), so an interpreter change moves it on its own. The instruction that used
/// to sit here — "restamp with the current `CARGO_PKG_VERSION`" — outlived the 2026-07-29 derivation
/// reform and would have had a reader edit a value that is not written by hand.
const DSL_INTERPRETER_FINGERPRINT: &str = FP_DSL;

/// The ruleset-fingerprint half of a file's `CacheKey`, over the already `is_enabled`-filtered pack set
/// `run_file_pass` computes once per `analyze_tree` call (see module doc for the composition and the
/// deviations from the spec's literal "serialized JSON" wording).
/// The disabled-rule ids that can reach a CACHED PER-FILE entry, and therefore the only ones whose
/// presence has to move [`ruleset_fingerprint`].
///
/// ## Why this is narrowed at all — a measurement, not a tidy-up
///
/// 📏 Disabling `circular` — a whole-graph rule that never touches a per-file entry — used to cost a
/// FULL cold run: 2,298 hits became 2,298 misses on `corpus/frameworks/nest` (2026-09-06, review ledger
/// V31). Every id went into the key, so toggling any rule invalidated every file. A team that keeps a
/// strict and a lenient config pays a cold run on every switch, and the reply says only `missFiles` —
/// never which ingredient moved. "Warm is a tenth of cold" never arrives for them.
///
/// ## Why narrowing is normally the WRONG move here, and what makes it safe in this one place
///
/// [`vocabulary_fingerprint`] refuses exactly this narrowing one function down, and its reason stands:
/// a per-lane subset is "a hand-maintained CLAIM about which lane consumes what, and nothing makes the
/// claim fail when a consumer moves". The two errors are not symmetric — over-invalidating costs a
/// recompute, under-invalidating serves a WRONG answer from a warm cache.
///
/// What is different here is that the claim is not hand-maintained. A cached per-file entry has exactly
/// TWO finding producers, `eval_packs` and `schema_findings` in `pipeline::fresh`, and
/// `scripts/check-cached-finding-producers.sh` fails if a third appears. So the set below is derived
/// from data this function already holds rather than asserted, and the assertion it does rest on is
/// machine-checked instead of remembered.
///
/// ## Over-inclusive on purpose, in the one direction that is free
///
/// The `schema` prefix sweeps in `schema-usage` and its ids, which run whole-tree and are NOT cached.
/// Keeping them costs an unnecessary recompute for anyone who disables them; dropping them would need a
/// second hand-kept distinction inside a family whose own umbrella ids are spelled in a crate this one
/// cannot enumerate. An id in no loaded pack and not schema-shaped — a typo, or a rule from a pack this
/// run did not load — cannot change a cached entry, and is correctly dropped.
pub(super) fn cache_relevant_disabled_rules(
    enabled_packs: &[&RulePackDef],
    disabled_rules: &[String],
) -> Vec<String> {
    let pack_rule_ids: std::collections::BTreeSet<&str> = enabled_packs
        .iter()
        .flat_map(|pack| pack.rules.iter().map(|r| r.id.as_str()))
        .collect();
    let mut kept: Vec<String> = disabled_rules
        .iter()
        .filter(|id| pack_rule_ids.contains(id.as_str()) || id.starts_with("schema"))
        .cloned()
        .collect();
    kept.sort();
    kept
}

pub(crate) fn ruleset_fingerprint(enabled_packs: &[&RulePackDef], config: &EngineConfig) -> String {
    let mut pack_parts: Vec<String> = enabled_packs
        .iter()
        .map(|pack| format!("{}\u{0}{pack:?}", pack.id))
        .collect();
    pack_parts.sort();

    let disabled_sorted =
        cache_relevant_disabled_rules(enabled_packs, &config.rule_config.disabled_rules);
    let disabled_json = serde_json::to_string(&disabled_sorted).unwrap_or_default();

    let schema_structural_fingerprint = schema_structural_fingerprint();
    let combined = format!(
        "{}\u{1}{disabled_json}\u{1}{schema_structural_fingerprint}\u{1}{DSL_INTERPRETER_FINGERPRINT}",
        pack_parts.join("\u{0}")
    );
    AnalysisCache::content_hash(combined.as_bytes())
}
