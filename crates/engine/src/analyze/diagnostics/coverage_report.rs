//! The coverage-gap diagnostics report (`run_diagnostics`) and its unknown-config-id substrates.

use zzop_core::DepGraph;
use zzop_metrics::{build_diagnostics, DiagnosticsInput, GitDiagnosticsInput};

use crate::EngineConfig;

/// `run_diagnostics`' result, split by output channel: `warnings` folds into
/// `AnalyzeOutput::warnings` (degenerate-output self-reports, plus the `unknown_suppression_rule_ids`
/// self-report — deliberately NOT split out, see that field's doc); `config_warnings` folds into
/// `AnalyzeOutput::config_warnings` instead — the `unknown_disabled_rule_ids`/
/// `unknown_severity_override_ids` self-reports (a typo'd `disabledRules`/`severityOverrides` entry did
/// nothing). Those two are config problems, not degenerate-output signals, so they belong on the same
/// honesty channel as `crates/config`'s parse-time config-mapper warnings even though THIS module (not
/// the config front-end) computes them — only analysis time has the known-rule-id set a config parser
/// never sees; this struct is the "merge at assembly" seam the caller (`assemble`/
/// `envelope::analyze_envelope`) folds into the two `AnalyzeOutput` fields.
pub(crate) struct DiagnosticsReport {
    pub(crate) warnings: Vec<String>,
    pub(crate) config_warnings: Vec<String>,
}

/// Builds `zzop_metrics::diagnostics`' coverage-gap self-report from data `assemble` already has in
/// scope — no extra pass. `symbols` filters on `SourceSymbol::exported` since `all_symbols` also
/// carries unexported top-level declarations. `concrete_modules`/`total_modules` are always `0` — no
/// real module classification is wired at this call site yet, and `0`/`0` is the honest "not measured"
/// value (the module's own `total_modules > 1` guard means that pair simply never fires until it is).
///
/// **Git-disabled gating**: `DiagnosticsInput::git` is `Option<GitDiagnosticsInput>` so the module
/// itself can tell "git was never attempted" (`None`) apart from "git ran and found zero" (`Some` with
/// honest zero counts) — `build_diagnostics` skips every git-window warning when `git` is `None`. This
/// passes `None` when `git_active` is `false`, `Some` with the honest counts otherwise.
pub(crate) fn run_diagnostics(
    file_count: usize,
    dep: &DepGraph,
    symbols: &[zzop_core::SourceSymbol],
    commits: &[zzop_core::CommitFileSet],
    config: &EngineConfig,
    git_active: bool,
) -> DiagnosticsReport {
    let dep_edges: u32 = dep.values().map(|targets| targets.len() as u32).sum();
    let exported_symbols = symbols.iter().filter(|s| s.exported).count() as u32;

    let git = git_active.then(|| {
        let (total_changes, tagged_changes, fix_changes) =
            commits
                .iter()
                .fold((0u32, 0u32, 0u32), |(total, tagged, fix), c| {
                    let n = c.files.len() as u32;
                    let tagged = tagged + if c.tags.is_empty() { 0 } else { n };
                    let fix = fix
                        + if c.tags.iter().any(|t| t == "FIX") {
                            n
                        } else {
                            0
                        };
                    (total + n, tagged, fix)
                });
        GitDiagnosticsInput {
            total_changes,
            tagged_changes,
            fix_changes,
            commits: commits.len() as u32,
            since: config.git.as_ref().and_then(|g| g.since.clone()),
        }
    });

    let diagnostics = build_diagnostics(DiagnosticsInput {
        files: file_count as u32,
        dep_edges,
        symbols: exported_symbols,
        concrete_modules: 0,
        total_modules: 0,
        git,
        unknown_disabled_rule_ids: unknown_disabled_rule_ids(config),
        unknown_severity_override_ids: unknown_severity_override_ids(config),
        unknown_suppression_rule_ids: unknown_suppression_rule_ids(config),
    });

    DiagnosticsReport {
        warnings: diagnostics.warnings,
        config_warnings: diagnostics.config_warnings,
    }
}

/// Every native-analysis id (built fresh here since the engine keeps no live `RuleRegistry` of its own)
/// plus, for each `config.packs` pack, every `"<pack>/<rule>"` id within it. Shared base for
/// `unknown_disabled_rule_ids`, `unknown_severity_override_ids`, and `unknown_suppression_rule_ids` —
/// `include_bare_pack_ids` controls the one place their "known" sets diverge (see call sites for why).
fn known_rule_ids(
    config: &EngineConfig,
    include_bare_pack_ids: bool,
) -> std::collections::HashSet<String> {
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut registry = zzop_core::RuleRegistry::new();
    crate::register_all_native(&mut registry);
    known.extend(registry.metas().iter().map(|m| m.id.clone()));
    for pack in &config.packs {
        if include_bare_pack_ids {
            known.insert(pack.id.clone());
        }
        for rule in &pack.rules {
            known.insert(format!("{}/{}", pack.id, rule.id));
        }
    }
    known
}

/// `RuleConfig::disabled_rules` entries that match no known rule id — the substrate for
/// `DiagnosticsInput::unknown_disabled_rule_ids`. "Known" is the union of every native-analysis id, every
/// `config.packs` pack id, and every `"<pack>/<rule>"` id within those packs — a bare pack id IS included
/// here because `registry::is_enabled`/`gate_pack_rules` both honor a bare pack id, dropping the whole pack
/// (see `is_enabled`'s doc).
fn unknown_disabled_rule_ids(config: &EngineConfig) -> Vec<String> {
    let known = known_rule_ids(config, true);
    config
        .rule_config
        .disabled_rules
        .iter()
        .filter(|id| !known.contains(id.as_str()))
        .cloned()
        .collect()
}

/// `RuleConfig::severity_overrides` entries that match no known rule id — the substrate for
/// `DiagnosticsInput::unknown_severity_override_ids`. "Known" here deliberately EXCLUDES bare pack ids,
/// unlike `unknown_disabled_rule_ids`: `registry::apply_severity_override` matches a finding's `rule_id`
/// exactly, and a DSL finding's `rule_id` is always `"<pack>/<rule>"` (see `dsl.rs`'s finding construction)
/// — a bare pack id can never equal a finding's `rule_id`, so treating it as "known" here would hide a
/// config entry that in fact remaps nothing.
fn unknown_severity_override_ids(config: &EngineConfig) -> Vec<String> {
    let known = known_rule_ids(config, false);
    config
        .rule_config
        .severity_overrides
        .keys()
        .filter(|id| !known.contains(id.as_str()))
        .cloned()
        .collect()
}

/// `RuleConfig::suppressions` entries whose `rule` matches no known rule id — the substrate for
/// `DiagnosticsInput::unknown_suppression_rule_ids`. Same narrower known-id union as
/// `unknown_severity_override_ids` (bare pack ids excluded): `registry::is_suppressed` matches a finding's
/// `rule_id` exactly against `entry.rule`, and a DSL finding's `rule_id` is always `"<pack>/<rule>"` (see
/// `dsl.rs`'s finding construction) — a bare pack id can never equal a finding's `rule_id`, so treating it
/// as "known" here would hide a suppression that in fact suppresses nothing. This check is independent of
/// `unmatched_suppression_warnings` (which flags a dead path/glob filter on an otherwise-valid rule id) —
/// a single `Suppression` entry can trigger both when its `rule` AND its filter are each wrong; that is
/// correct, they are orthogonal diagnostics over the same entry.
fn unknown_suppression_rule_ids(config: &EngineConfig) -> Vec<String> {
    let known = known_rule_ids(config, false);
    config
        .rule_config
        .suppressions
        .iter()
        .filter(|s| !known.contains(s.rule.as_str()))
        .map(|s| s.rule.clone())
        .collect()
}

/// `AnalyzeOutput::rule_overrides_applied`'s substrate (D13③) — the positive complement of
/// `unknown_disabled_rule_ids`/`unknown_severity_override_ids` just above: requested ids that DID match a
/// known id, so the disable/remap actually took effect this run. Reuses the exact same `known_rule_ids`
/// sets those two functions use (`true`/`false` `include_bare_pack_ids` splits identically, for the same
/// reasons documented on each) — never a second "known" definition to drift out of sync.
///
/// `None` when neither `disabled_rules` nor `severity_overrides` has any entry — nothing was requested,
/// so there is nothing to positively confirm (the quieter of the two conventions
/// `AnalyzeOutput::rule_overrides_applied`'s own doc allows). A request that matched NOTHING (every entry
/// a typo) still yields `Some` with empty lists rather than `None`: something was requested, and
/// confirming "none of it took effect" is exactly this field's job — collapsing that into `None` would
/// make it indistinguishable from "the caller never touched these knobs".
pub(crate) fn rule_overrides_applied(config: &EngineConfig) -> Option<crate::RuleOverridesApplied> {
    if config.rule_config.disabled_rules.is_empty()
        && config.rule_config.severity_overrides.is_empty()
    {
        return None;
    }
    let known_with_bare = known_rule_ids(config, true);
    let known_without_bare = known_rule_ids(config, false);

    let mut disabled: Vec<String> = config
        .rule_config
        .disabled_rules
        .iter()
        .filter(|id| known_with_bare.contains(id.as_str()))
        .cloned()
        .collect();
    disabled.sort();
    disabled.dedup();

    let mut severity_remapped: Vec<String> = config
        .rule_config
        .severity_overrides
        .keys()
        .filter(|id| known_without_bare.contains(id.as_str()))
        .cloned()
        .collect();
    severity_remapped.sort();
    severity_remapped.dedup();

    Some(crate::RuleOverridesApplied {
        disabled,
        severity_remapped,
    })
}
