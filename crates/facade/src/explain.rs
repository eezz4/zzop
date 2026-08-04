//! `zzop explain <rule-id>` — a read-only lookup straight from the DSL rule data compiled INTO this
//! binary (`zzop_config::BUNDLED_PACK_SOURCES`, parsed with the same loader path the engine uses:
//! `zzop_core::parse_dsl_pack`), so the answer can never drift from what the engine actually runs.
//! NEVER reads `docs/rules/catalog.md` prose. CLI-only: MCP already reaches the same rule data through
//! the `rule-catalog` embedded-contract resource (`zzop://contract/rule-catalog`,
//! `zzop_summary::contracts`), so this has no `tools/call` twin.
//!
//! Accepted id forms: the full `<pack>/<rule>` id every finding's `ruleId` already carries, and a bare
//! `<rule>` id when it is unambiguous across every bundled pack (checked in that order — a full-form
//! match is authoritative even when the bare id alone would also be ambiguous). Four further cases are
//! lookup FAILURES, each with its own message so the caller is never left guessing which kind of
//! "not explainable" they hit:
//! - the id names a whole PACK, not a rule within one (its rule ids are printed as a hint);
//! - the id is a native analysis id (`circular`, `duplicate-route`, `cross-layer/*`, `schema/*`, ... —
//!   compiled into `zzop-engine`, never a bundled DSL pack) — real, just not data this lookup reads;
//! - the id is the BARE form of a namespaced native id (`god-model`, `route-near-miss`) — see below;
//! - the id is an OUTPUT ID that is not a rule id at all — a coverage-disclosure class or group
//!   (`disclosure[].id` / `.group`) or a recommendation id (`architecture.topRecommendation.id`), all
//!   three printed under a field literally named `id`/`group` in every analyze reply (see
//!   [`output_ids`]);
//! - the id is unknown outright — pointed at `zzop contract rule-catalog` for the full prose list.
//!
//! ## The bare-native-id lane
//! Two native families namespace their ids with a `/`: `cross-layer/*` and `schema/*`. A reader who types
//! the tail alone (`god-model`, `route-near-miss`) means something real, but `disabledRules` /
//! `severityOverrides` match EXACTLY, so the bare string would configure nothing. [`bare_native_matches`]
//! resolves it against the live registry and the answer names the full id to use — the same courtesy the
//! bare DSL lane above extends, on the same terms (only when exactly one registered id ends in it), and
//! deliberately after the exact-match lane so `duplicate-route` still resolves to the bare id it names
//! rather than to `cross-layer/duplicate-route`.
//!
//! This replaced an ISSUE-LABEL lane. `schema` findings used to carry no registered rule id at all:
//! `ruleId` was composed as `schema/<label>` from a per-issue label while only the two family gates
//! (`schema-structural`, `schema-usage`) were registered, so the exact string a user copies out of real
//! output landed in the WORST lane ("unknown rule id", exit 1) and that lane existed to soften it. The 12
//! labels are registered ids now (`zzop_rules_schema::register_native_analyses`), so the softening lane is
//! gone rather than kept as a second, now-false explanation: the namespaced form is answered by the
//! native-id lane and the bare form by this one.
//!
//! ## Scope, censused rather than assumed
//! Every identifier a user can read out of real output was enumerated and fed to this lookup: the DSL pack
//! ids, every DSL rule in both the full and the bare form, every registered native analysis id (bare AND,
//! for the namespaced families, its tail), the recommendation ids, the disclosure classes and their
//! groups, and the distinct matcher `label`s the DSL packs declare. After the lanes above, everything
//! printed under an `id`/`ruleId`/`group` field is answered. The counts are deliberately NOT restated
//! here — `docs/rules/catalog.md`'s totals line is machine-checked against the loaded packs and the
//! registry, and a second hand-kept copy in this doc comment would be the drift this file keeps finding.
//!
//! What deliberately still answers "unknown rule id" is one class: the sub-labels that ride a finding's
//! `data` under an id that is ALREADY registered and already explainable — a line-scan match's
//! `data.label` (`sink`, `write`, `guard`, ...), `unimported-export`'s `data.reason`
//! (`unused`/`in-file-only`),
//! and `cross-layer/route-near-miss`' `data.dimension` (`case`/`prefix`). None of them is an id: they are
//! fields of an explainable finding, the finding names its own `ruleId` one line away, and nearly all
//! matcher labels are ordinary English words (`read`, `set`, `body`, `timeout`) that would turn `explain`
//! into a dictionary of vocabulary that does not exist as a rule id anywhere.

#[cfg(test)]
mod field_coverage_tests;
mod output_ids;
mod render;
mod scope;
#[cfg(test)]
mod tests;

use render::render;
use zzop_core::{RuleDef, RulePackDef, RuleRegistry};

/// `zzop explain <rule-id>` — `Ok` is the rendered rule text (print to stdout, exit 0), `Err` is a
/// caller-facing message for one of the three lookup-failure lanes described in the module doc (print
/// to stderr, exit 1). Loads the real bundled packs and the real native-analysis registry fresh on
/// every call — a single lookup is not worth caching across the process lifetime of a one-shot CLI run.
pub fn explain(query: &str) -> Result<String, String> {
    explain_over(&bundled_packs(), &native_analysis_ids(), query)
}

/// Every bundled DSL pack, parsed fresh with the exact loader path the engine itself uses
/// (`zzop_core::parse_dsl_pack` over `zzop_config::BUNDLED_PACK_SOURCES`) — see the sibling
/// `crate::envelope`'s `bundled_pack_defs` for the twin of this loop. The two are deliberately NOT one
/// helper even now that they are modules of one crate: that one seeds an ANALYSIS (a pack that fails to
/// parse becomes a caller-visible warning on the run's warnings channel), while this one answers a
/// LOOKUP — a pack that fails to parse is skipped silently, because `explain` is a best-effort read, not
/// a load-time gate (that gate already lives at `validate-rule-pack` and the engine's own boot path). A
/// shared helper would have to take the divergent failure handling as a parameter, which is the whole
/// body.
fn bundled_packs() -> Vec<RulePackDef> {
    zzop_config::BUNDLED_PACK_SOURCES
        .iter()
        .filter_map(|(_rel_path, source)| zzop_core::parse_dsl_pack(source).ok())
        .collect()
}

/// Every native analysis id compiled into `zzop-engine` (`circular`, `cross-layer/route-shadowing`,
/// ...) — read off the real registry `zzop_engine::register_all_native` populates, never a hand-copied
/// list, so the "this id is native, not missing" lane can't drift from what the engine actually
/// registers either.
fn native_analysis_ids() -> Vec<String> {
    let mut registry = RuleRegistry::new();
    zzop_engine::register_all_native(&mut registry);
    registry.ids().to_vec()
}

/// Every registered native analysis id whose tail after the single `/` equals `query` — the bare form of a
/// NAMESPACED native id (`god-model` for `schema/god-model`, `route-near-miss` for
/// `cross-layer/route-near-miss`). Returned as a list because the caller must tell "exactly one" from
/// "several" and answer differently, exactly as the bare DSL lane above it does.
fn bare_native_matches<'a>(native_ids: &'a [String], query: &str) -> Vec<&'a String> {
    native_ids
        .iter()
        .filter(|id| id.rsplit_once('/').is_some_and(|(_, tail)| tail == query))
        .collect()
}

/// The pure lookup, parameterized on its two data sources so it is testable against a fabricated pack
/// list (real bundled data has zero bare-id collisions today — `derived_suppress_markers_are_globally_
/// unique` in `crates/engine/tests/rule_contracts/markers.rs` machine-enforces exactly that — so the
/// ambiguous-bare-id lane below has no REAL trigger to pin an end-to-end test against; see its unit test
/// for the fabricated-collision case this reaches for).
fn explain_over(
    packs: &[RulePackDef],
    native_ids: &[String],
    query: &str,
) -> Result<String, String> {
    // Full `<pack>/<rule>` form, checked first: an exact `pack.id/rule.id` match resolves
    // deterministically even on a bare id that would also be ambiguous elsewhere.
    for pack in packs {
        for rule in &pack.rules {
            if format!("{}/{}", pack.id, rule.id) == query {
                return Ok(render(pack, rule));
            }
        }
    }

    // Bare `<rule>` form — accepted only when unambiguous across every bundled pack.
    let bare_matches: Vec<(&RulePackDef, &RuleDef)> = packs
        .iter()
        .flat_map(|pack| pack.rules.iter().map(move |rule| (pack, rule)))
        .filter(|(_, rule)| rule.id == query)
        .collect();
    if bare_matches.len() == 1 {
        let (pack, rule) = bare_matches[0];
        return Ok(render(pack, rule));
    }
    if bare_matches.len() > 1 {
        let mut ids: Vec<String> = bare_matches
            .iter()
            .map(|(pack, rule)| format!("{}/{}", pack.id, rule.id))
            .collect();
        ids.sort();
        return Err(format!(
            "rule id {query:?} is ambiguous across {} bundled packs — use the full id: {}",
            ids.len(),
            ids.join(", ")
        ));
    }

    // Names a whole PACK, not a rule within one — a legitimate id, just not a single explainable rule.
    if let Some(pack) = packs.iter().find(|pack| pack.id == query) {
        let mut rule_ids: Vec<String> = pack
            .rules
            .iter()
            .map(|rule| format!("{}/{}", pack.id, rule.id))
            .collect();
        rule_ids.sort();
        return Err(format!(
            "{query:?} is a rule PACK, not a single rule — explain one of its rules instead: {}",
            rule_ids.join(", ")
        ));
    }

    // A native analysis id (compiled into zzop-engine, not a bundled DSL pack) — real, just not data
    // this lookup reads.
    if native_ids.iter().any(|id| id == query) {
        return Err(format!(
            "{query:?} is a native analysis id, not a bundled DSL rule — `zzop explain` only reads the \
             compiled-in DSL pack data. See `zzop contract rule-catalog` for its full prose entry."
        ));
    }

    // A NAMESPACED native id typed bare — `god-model` for `schema/god-model`, `route-near-miss` for
    // `cross-layer/route-near-miss`. Same terms as the bare DSL lane above (accepted only when
    // unambiguous), and deliberately AFTER the exact-match lane, so `duplicate-route` — registered both
    // bare (`zzop_rules_http`) and as `cross-layer/duplicate-route` — resolves to the id it literally
    // names rather than being called ambiguous.
    match bare_native_matches(native_ids, query).as_slice() {
        [full] => {
            return Err(format!(
                "{query:?} is the bare form of the native analysis id {full:?} (compiled into \
                 zzop-engine, not a bundled DSL pack) — real, just not data this lookup reads. Config \
                 matches ids EXACTLY, so `disabledRules` / `severityOverrides` need the full \
                 {full:?}. See `zzop contract rule-catalog` for its full prose entry."
            ));
        }
        [] => {}
        several => {
            let mut ids: Vec<&str> = several.iter().map(|id| id.as_str()).collect();
            ids.sort();
            return Err(format!(
                "native analysis id {query:?} is ambiguous across {} namespaces — use the full id: {}",
                ids.len(),
                ids.join(", ")
            ));
        }
    }

    // An OUTPUT ID that is not a rule id at all — a disclosure class/group or a recommendation id. Last
    // of the tailored lanes: every id-shaped lane above names something the rule surface owns, this one
    // names something only the OUTPUT owns (`explain`'s own module doc, "Scope, censused").
    if let Some(message) = output_ids::output_id_lane(query) {
        return Err(message);
    }

    Err(format!(
        "unknown rule id {query:?} — see `zzop contract rule-catalog` for the full list of rule ids."
    ))
}
