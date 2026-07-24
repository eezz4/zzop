//! `zzop explain <rule-id>` — a read-only lookup straight from the DSL rule data compiled INTO this
//! binary (`zzop_config::BUNDLED_PACK_SOURCES`, parsed with the same loader path the engine uses:
//! `zzop_core::parse_dsl_pack`), so the answer can never drift from what the engine actually runs.
//! NEVER reads `docs/rules/catalog.md` prose. CLI-only: MCP already reaches the same rule data through
//! the `rule-catalog` embedded-contract resource (`zzop://contract/rule-catalog`,
//! `zzop_host::embedded`), so this has no `tools/call` twin and lives outside `tools.rs`.
//!
//! Accepted id forms: the full `<pack>/<rule>` id every finding's `ruleId` already carries, and a bare
//! `<rule>` id when it is unambiguous across every bundled pack (checked in that order — a full-form
//! match is authoritative even when the bare id alone would also be ambiguous). Three further cases are
//! lookup FAILURES, each with its own message so the caller is never left guessing which kind of
//! "not explainable" they hit:
//! - the id names a whole PACK, not a rule within one (its rule ids are printed as a hint);
//! - the id is a native analysis id (`circular`, `duplicate-route`, `cross-layer/*`, ... — compiled into
//!   `zzop-engine`, never a bundled DSL pack) — real, just not data this lookup reads;
//! - the id is unknown outright — pointed at `zzop contract rule-catalog` for the full prose list.

#[cfg(test)]
mod tests;

use zzop_core::{Matcher, RuleDef, RulePackDef, RuleRegistry, Severity};

/// `zzop explain <rule-id>` — `Ok` is the rendered rule text (print to stdout, exit 0), `Err` is a
/// caller-facing message for one of the three lookup-failure lanes described in the module doc (print
/// to stderr, exit 1). Loads the real bundled packs and the real native-analysis registry fresh on
/// every call — a single lookup is not worth caching across the process lifetime of a one-shot CLI run.
pub fn explain(query: &str) -> Result<String, String> {
    explain_over(&bundled_packs(), &native_analysis_ids(), query)
}

/// Every bundled DSL pack, parsed fresh with the exact loader path the engine itself uses
/// (`zzop_core::parse_dsl_pack` over `zzop_config::BUNDLED_PACK_SOURCES`) — see
/// `zzop_facade::envelope::bundled_pack_defs` for the established twin of this loop (private to
/// `zzop-facade`, feeding the envelope-mode analysis default; this copy feeds `explain` alone, so it is
/// not worth threading a shared helper across the crate boundary for one caller each). A pack that
/// fails to parse — impossible for a committed bundled pack unless the embed itself is broken — is
/// skipped silently: `explain` is a best-effort lookup, not a load-time gate (that gate already lives at
/// `validate-rule-pack` and the engine's own boot path).
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
    registry.metas().iter().map(|m| m.id.clone()).collect()
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

    Err(format!(
        "unknown rule id {query:?} — see `zzop contract rule-catalog` for the full list of rule ids."
    ))
}

/// The stable-order, human-readable rendering: full id, pack, severity, message, the DERIVED suppress
/// marker (`RuleDef::suppress_marker()` — never stored, see its own doc), matcher kind, and whether the
/// rule carries a line-scan `exclude_pattern`.
fn render(pack: &RulePackDef, rule: &RuleDef) -> String {
    [
        format!("id: {}/{}", pack.id, rule.id),
        format!("pack: {}", pack.id),
        format!("severity: {}", severity_str(rule.severity)),
        format!("message: {}", rule.message),
        format!("suppress marker: {}", suppress_marker_str(rule)),
        format!("matcher: {}", matcher_kind(&rule.matcher)),
        format!("exclude_pattern: {}", has_exclude_pattern(&rule.matcher)),
    ]
    .join("\n")
}

fn severity_str(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "critical",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

/// One of the four matcher shapes `docs/rules/dsl-reference.md` documents — the serde tag names
/// themselves (`Matcher`'s `#[serde(tag = "type", rename_all = "kebab-case")]`), so this can never drift
/// from what a pack's own `"type"` field spells.
fn matcher_kind(matcher: &Matcher) -> &'static str {
    match matcher {
        Matcher::LineScan(_) => "line-scan",
        Matcher::MethodScan(_) => "method-scan",
        Matcher::SymbolScan(_) => "symbol-scan",
        Matcher::IoScan(_) => "io-scan",
    }
}

/// The derived marker, but only for the matcher kinds that actually honor one: `symbol-scan` findings have
/// no source line to anchor a comment against, so no marker can ever suppress them
/// (`docs/rules/dsl-reference.md`'s "Suppress-marker semantics"). Printing `<id>-ok` there would hand the
/// reader a comment that silently does nothing. Latent today — no bundled pack uses `symbol-scan` — which is
/// exactly why it is worth answering honestly before the first one ships.
fn suppress_marker_str(rule: &RuleDef) -> String {
    match rule.matcher {
        Matcher::SymbolScan(_) => {
            "none (symbol-scan findings have no line to anchor a marker)".to_string()
        }
        _ => rule.suppress_marker(),
    }
}

/// Only `LineScan` carries an `exclude_pattern` field at all (`MethodScan` has `absent` instead;
/// `SymbolScan`/`IoScan` have neither) — every other matcher kind answers `no` here, honestly, not
/// "not applicable".
fn has_exclude_pattern(matcher: &Matcher) -> &'static str {
    match matcher {
        Matcher::LineScan(m) if m.exclude_pattern.is_some() => "yes",
        _ => "no",
    }
}
