//! Mode A's whole-graph native analyses — `circular`/`unreachable`/`dead-candidates`, extracted
//! verbatim from `ingest::analyze_envelope` when that file crossed the 300-line ceiling
//! (`check-max-file-lines`). Behavior is identical to the pre-split inline block: same `is_enabled`
//! gates, same `record_native_timing` ids, same inputs, same finding order.

use std::collections::{HashMap, HashSet};

use zzop_core::{is_enabled, Finding, NormalizedEnvelope};

use crate::analyze::{circular_findings, dead_candidate_findings, unreachable_findings};
use crate::EngineConfig;

/// Runs the three whole-graph native analyses over the envelope's assembled graph, timed exactly
/// like `assemble::rules::run`'s own gates (same `record_native_timing`, same ids) — a Mode A
/// profile ranks the same analysis classes.
pub(super) fn run_whole_graph_native(
    envelope: &NormalizedEnvelope,
    config: &EngineConfig,
    cycles: &[Vec<String>],
    nodes: &[zzop_core::FileNode],
    dep: &zzop_core::DepGraph,
    rule_time: &mut HashMap<String, (u128, usize)>,
) -> Vec<Finding> {
    let profile = config.profile_rules;
    let mut global_findings = Vec::new();
    if is_enabled(&config.rule_config, "circular") {
        let t0 = profile.then(std::time::Instant::now);
        let found = circular_findings(cycles);
        crate::analyze::record_native_timing(rule_time, t0, "circular", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "unreachable") {
        // No filesystem root here (see the envelope module doc), so there are no cargo manifests to
        // scan for declared-target entries — the empty set is the honest Mode-A value, same rationale
        // as `dead-candidates`' empty package.json entry set just below.
        let t0 = profile.then(std::time::Instant::now);
        let found = unreachable_findings(nodes, dep, &Default::default());
        crate::analyze::record_native_timing(rule_time, t0, "unreachable", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "dead-candidates") {
        // No filesystem root (see the envelope module doc) -> no package.json-referenced entries; the
        // envelope's own `is_entry`-marked projections ARE the entry set — the Mode A counterpart of the
        // Mode B overlay union in `analyze::assemble` (same contract marker, same exemption). Before
        // this, Mode A silently dropped `is_entry` and every convention-loaded entry file (a crate's
        // `lib.rs`, a test harness file) read as dead — caught by a Mode A envelope example's
        // self-analysis.
        let extra_entries: HashSet<String> = envelope
            .files
            .iter()
            .filter(|f| f.is_entry)
            .map(|f| f.path.clone())
            .collect();
        // Deliberate divergence from the native `assemble::rules` path: it post-filters out generated
        // (`@generated`/auto-generated-bannered) files via `generated_banner::file_has_generated_banner`,
        // which re-reads each candidate's head off disk. Mode A has no filesystem `root` and a
        // `FileProjection` (normalized.rs) carries no raw text, so that head-comment detector structurally
        // cannot run here — an adapter that wants a generated file exempt marks it `is_entry` (above) or
        // omits it from the envelope. Documented, not a bug: the exemption is a native-path-only refinement.
        let t0 = profile.then(std::time::Instant::now);
        let found = dead_candidate_findings(nodes, dep, &extra_entries);
        crate::analyze::record_native_timing(rule_time, t0, "dead-candidates", found.len());
        global_findings.extend(found);
    }
    global_findings
}
