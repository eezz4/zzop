//! Mode A warning assembly — the config-derived self-report block extracted verbatim from
//! `ingest::analyze_envelope` (file-line cap), same order: reserved-channel drop note, zero-packs,
//! the git-skip note, config-diagnostics parity, unmatched suppression/global-exclude, the
//! pack-scope census pair, and the uncompilable-rule disclosure. Returns the census alongside the
//! config warnings because `packs_loaded` must read the SAME `DslScope` instance these warnings
//! reasoned about — one census, two consumers (see `compute_dsl_scope`).

use zzop_core::{DepGraph, NormalizedEnvelope};

use crate::analyze::DslScope;
use crate::EngineConfig;

use super::reserved::reserved_drop_warning;

/// Appends every envelope-mode self-report to `warnings` and returns
/// `(config_warnings, dsl_scope)` — see the module doc for the fixed order and why the census is
/// returned. `rels` is the analyzed-file list (`loc_by_path` keys); `reserved_dropped` comes from
/// the file pass.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_envelope_warnings(
    envelope: &NormalizedEnvelope,
    config: &EngineConfig,
    file_count: usize,
    dep: &DepGraph,
    all_symbols: &[zzop_core::SourceSymbol],
    rels: &[&str],
    reserved_dropped: usize,
    warnings: &mut Vec<String>,
) -> (Vec<String>, DslScope) {
    if let Some(w) = reserved_drop_warning("envelope", &envelope.parser, reserved_dropped) {
        warnings.push(w);
    }
    if let Some(w) = crate::analyze::zero_packs_warning(config) {
        warnings.push(w);
    }
    // Same self-report as the tree lane's (`analyze::assemble::dep_graph`) — an unusable declared
    // test-path pattern is silent in exactly the direction that looks like a clean run.
    warnings.extend(crate::vocabulary::extra_test_path_tail(&config.vocabulary).1);
    if config.git.is_some() {
        warnings.push(
            "git collection skipped: envelope mode has no filesystem root to collect history from"
                .to_string(),
        );
    }
    // Config-diagnostics parity with `analyze::assemble` — the envelope path used to skip these, so a
    // `disabled_rules` typo or a dead suppression/top-level-exclude filter was silently ineffective in
    // envelope mode only (the "envelope diagnostics asymmetry" gap). `commits` is empty and `git_active`
    // false: envelope mode never has history, and `build_diagnostics` skips every git-window warning on
    // that gate, so only the structural coverage-gap + unknown-`disabled_rules` self-reports fire.
    let diagnostics_report =
        crate::analyze::run_diagnostics(file_count, dep, all_symbols, &[], config, false);
    warnings.extend(diagnostics_report.warnings);
    let config_warnings = diagnostics_report.config_warnings;
    warnings.extend(crate::analyze::unmatched_suppression_warnings(config, rels));
    warnings.extend(crate::analyze::unmatched_global_exclude_warnings(
        config, rels,
    ));
    // One census, two consumers (see `compute_dsl_scope`): the warning below and `packs_loaded`'s counts.
    // FILTERED by the same matcher-kind predicate `envelope_rule_pack` evaluates with, so a rule this
    // mode never runs (line-scan/method-scan/call-scan/literal-scan) is listed in `zeroAdmissionRules`
    // — its green is vacuous here — instead of reading as covered because its path pattern matched.
    let dsl_scope = crate::analyze::compute_dsl_scope_filtered(
        &config.packs,
        rels,
        super::resolve::rule_runs_in_envelope_mode,
    );
    // Both of that census's reports are config-derived, so Mode A gets the identical pair Mode B does.
    warnings.extend(crate::analyze::pack_scope_warnings(config, &dsl_scope));
    // Same disclosure Mode B's native twin makes (`analyze::assemble`) — an uncompilable rule is dead in
    // envelope mode too, and a caller who injected the pack inline never ran `validate-rule-pack` on it.
    warnings.extend(crate::analyze::uncompilable_rule_warnings(&config.packs));
    // Config-derived like the disclosure above it, so Mode A gets it too: a caller who seeded a pack
    // inline (or pointed at a `packsDir`) whose rule id collides with a bundled one has the identical
    // silent co-suppression hazard here — the marker is derived from the pack set, not from the tree.
    warnings.extend(zzop_core::suppress_marker_collisions(&config.packs));
    (config_warnings, dsl_scope)
}
