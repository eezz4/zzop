//! `zzop explain <rule-id> [--config <path>]` — a read-only lookup straight from the DSL rule data the
//! engine itself would run, parsed with the same loader path (`zzop_core::parse_dsl_pack`), so the
//! answer can never drift from what actually runs. NEVER reads `docs/rules/catalog.md` prose. CLI-only:
//! MCP already reaches the same rule data through the `rule-catalog` embedded-contract resource
//! (`zzop://contract/rule-catalog`, `zzop_summary::contracts`), so this has no `tools/call` twin.
//!
//! WHICH rule data is the [`Corpus`] axis, and it is the one thing a caller chooses. Bare [`explain`]
//! reads the packs compiled into this binary (`zzop_config::BUNDLED_PACK_SOURCES`).
//! [`explain_with_config`] reads the packs a CONFIG's trees actually load — the bundled ones plus every
//! `zzop/rules/` and `packs.extraDirs` directory those trees name. See [`from_config`] for why the
//! second form had to exist and why it is not the default.
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
//! - the id is unknown outright — pointed at `zzop contract rule-catalog` for the full prose list, plus
//!   the [`Corpus`]-specific tail (a bundled-only lookup names `--config`, because "unknown" there is
//!   the exact answer a retrieved pack's rule id gets and the reader has no other way to learn why).
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
mod from_config;
mod output_ids;
mod render;
mod scope;
#[cfg(test)]
mod tests;

pub use from_config::explain_with_config;
use render::render;
use zzop_core::{RuleDef, RulePackDef, RuleRegistry};

/// Which pack corpus a lookup searched. Read ONLY by the unknown-id lane, to shape its tail — every
/// other lane's message is corpus-independent, because every other lane names something the query
/// itself is (a pack, a native id, an output id) rather than something the search failed to contain.
#[derive(Clone, Copy)]
pub(crate) enum Corpus<'a> {
    /// The packs compiled into this binary — [`explain`].
    Bundled,
    /// The packs a config's trees load — [`explain_with_config`]. Carries the config path so the
    /// failure names the file whose pack set was actually searched, not "a config": pointing at the
    /// wrong config is itself a way to get "unknown" for a rule that does load.
    Config(&'a str),
}

/// How to reach the rule catalog, named for BOTH surfaces that print these errors.
///
/// 🔴 These strings are built once in the facade and printed by the CLI *and* by the MCP server, so a
/// pointer written in one surface's vocabulary is an instruction the other surface's reader cannot
/// follow. Measured (external review round 20, ledger V219): `resources/read` on
/// `zzop://rule/mutating-route-no-auth` returned a `-32602` whose text was *"See `zzop contract
/// rule-catalog`"* — a shell command, handed to a client that has no shell. The document itself was
/// reachable the whole time as `zzop://contract/rule-catalog`, and is listed in `resources/list`; only
/// the sentence naming it was wrong.
///
/// Naming both is deliberately preferred over threading a surface flag through this crate: an error
/// path is the one place a reader is already stuck, one extra clause is cheap there, and a flag would
/// put the "which surface am I" question into every caller of a shared text.
const RULE_CATALOG_BOTH_WAYS: &str =
    "`zzop contract rule-catalog` on the CLI, or the `zzop://contract/rule-catalog` resource over MCP.";

/// `zzop explain <rule-id>` — `Ok` is the rendered rule text (print to stdout, exit 0), `Err` is a
/// caller-facing message for one of the lookup-failure lanes described in the module doc (print
/// to stderr, exit 1). Loads the real bundled packs and the real native-analysis registry fresh on
/// every call — a single lookup is not worth caching across the process lifetime of a one-shot CLI run.
pub fn explain(query: &str) -> Result<String, String> {
    explain_over(
        &bundled_packs(),
        &native_analysis_ids(),
        query,
        Corpus::Bundled,
    )
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
/// The assembled message a BUNDLED rule would have produced, keyed by full `"<pack>/<rule>"` id —
/// the rule's own declared `message` plus the two sentences the engine appends
/// (`zzop_core::dsl::message_with_hints`, the same function `pipeline::findings::append_hints` calls).
///
/// # Why this exists, and why the population is BUNDLED and not "every loaded pack"
/// `zzop-summary` replaces a message equal to this with `BY_ID_MESSAGE` and tells the reader to
/// resolve it by `ruleId` through `zzop explain <id>` or the MCP `zzop://rule/{id}` resource. Both of
/// those answer out of [`bundled_packs`] — the packs compiled into this binary — and NOTHING ELSE:
/// `explain` without `--config` cannot see a pack from `zzop/rules/` or `packs.extraDirs`, and the MCP
/// resource has no config axis at all to give it one (`resources/templates/list` publishes
/// `zzop://rule/{id}` and no second parameter). So a finding from a user pack that carried the pointer
/// would name a door that answers "unknown rule id" — measured, round 18: a repo pack in `zzop/rules/`
/// produced 6 findings, all 6 pointered, and both `zzop explain <id>` (exit 1) and
/// `resources/read zzop://rule/<id>` (-32602) refused them.
///
/// 🔴 **`EngineConfig`'s `PackSource` is NOT the right discriminator for this**, which is the obvious
/// wrong answer: its own doc says there is deliberately no `Bundled` variant, because "bundled" is a
/// packaging fact of the host and the engine sees only a directory or an inline def. A bundled pack
/// arrives as `Dir` whenever the mapper prepends the bundled directory to `packsDir`. The property
/// that actually matters is not "where did this pack come from" but "can the door open for this id",
/// and that question has exactly one honest answer: ask the same corpus the door asks.
///
/// Built once. The corpus is compile-time (`zzop_config::BUNDLED_PACK_SOURCES`), which is also what
/// lets the shortening live in the shaper rather than needing a run's `config.packs` at the seam.
pub fn bundled_verbatim_message(rule_id: &str) -> Option<&'static str> {
    static TABLE: std::sync::OnceLock<std::collections::HashMap<String, String>> =
        std::sync::OnceLock::new();
    TABLE
        .get_or_init(|| {
            let mut out = std::collections::HashMap::new();
            for pack in bundled_packs() {
                for rule in &pack.rules {
                    let id = format!("{}/{}", pack.id, rule.id);
                    let text = zzop_core::dsl::message_with_hints(Some(rule), &id, &rule.message);
                    out.insert(id, text);
                }
            }
            out
        })
        .get(rule_id)
        .map(String::as_str)
}

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
///
/// `pub` since 2026-08-12 for a SECOND caller with the same question: `zzop-summary`'s findings-view
/// filter has to tell "no findings for this rule" from "no such rule", and a bare (`/`-less) `--rule`
/// argument can only ever be a native id — a DSL finding's `rule_id` is always `"<pack>/<rule>"`. The
/// summary crate is layered above the facade and must not reach past it to `zzop-engine` (see
/// `lib.rs`'s re-export doc for the same rule applied to `disclosure_counts`/`SCORE_MEANINGS`), so the
/// registry read lives here and is exported rather than duplicated there.
pub fn native_analysis_ids() -> Vec<String> {
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
    corpus: Corpus<'_>,
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
            "{query:?} is a native analysis id, not a bundled DSL rule — this lookup only reads the \
             compiled-in DSL pack data. Its full prose entry is in the rule catalog: {RULE_CATALOG_BOTH_WAYS}"
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
                 {full:?}. Its full prose entry is in the rule catalog: {RULE_CATALOG_BOTH_WAYS}"
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
        "unknown rule id {query:?} — the full list of rule ids is in the rule catalog: \
         {RULE_CATALOG_BOTH_WAYS}{}",
        match corpus {
            // The tail that closes the retrieval loop. A rule that left the bundle and was recovered
            // into `zzop/rules/` RUNS — it appears in `packsLoaded` and its findings carry this exact
            // id — while this lookup, reading only compiled-in packs, calls the id unknown. Without
            // this sentence the two surfaces simply disagree and the reader has nothing to go on.
            Corpus::Bundled =>
                " This searched only the packs compiled into this binary; a rule from a pack in \
                 `zzop/rules/` or `packs.extraDirs` is not among them — re-run as `zzop explain \
                 <rule-id> --config <path>` to search the packs a run over that config actually loads."
                    .to_string(),
            // The symmetric answer once they HAVE done that: the remaining ways to still miss are a
            // config that names other trees, or a pack that failed to load (a warning the analyze
            // reply carries and this lookup deliberately does not — see `bundled_packs`' doc).
            // Backticks, not `{path:?}`: a Windows path debug-formats with every separator escaped
            // (`C:\\Users\\...`), which is not a path the reader can copy back into a command.
            Corpus::Config(path) => format!(
                " The packs `{path}` loads were searched too — `packsLoaded` in that config's `zzop \
                 analyze` reply names every pack that did load, and its `warnings` name any that \
                 failed to."
            ),
        }
    ))
}
