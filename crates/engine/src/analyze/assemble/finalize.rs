//! The phase AFTER findings — scores, diagnostics, the fused IR/coverage census, and the git window.
//!
//! Split out of the parent orchestrator for size, and the seam is the same one `graph_build` uses in the
//! call-graph pass: everything here runs on facts that are already FINAL. Nothing in this module can
//! produce or suppress a finding, which is exactly why it can be read (and changed) without re-deriving
//! what the analysis decided.
//!
//! The input is a struct rather than a parameter list for the reason `fuse::FuseInputs` already states in
//! this same directory: at this width a positional call site stops saying what each value is.

use super::{diagnose, fuse, metrics};
use crate::analyze::diagnostics::run_diagnostics;
use crate::EngineConfig;
use std::collections::HashMap;
use zzop_core::Finding;
use zzop_core::ImportMap;

pub(super) struct FinalizeInputs<'a> {
    pub(super) root: &'a std::path::Path,
    pub(super) config: &'a EngineConfig,
    pub(super) nodes: &'a [zzop_core::FileNode],
    pub(super) dep: zzop_core::DepGraph,
    pub(super) cycles: Vec<Vec<String>>,
    pub(super) commits: Vec<zzop_core::CommitFileSet>,
    pub(super) git_active: bool,
    pub(super) findings: &'a [Finding],
    pub(super) rule_time: HashMap<String, (u128, usize)>,
    pub(super) file_count: usize,
    pub(super) all_symbols: Vec<zzop_core::SourceSymbol>,
    pub(super) loc_by_path: HashMap<String, u32>,
    pub(super) io: Option<zzop_core::IoFacts>,
    pub(super) parser_dispatched: usize,
    pub(super) degraded: usize,
    pub(super) ts_import_pairs: &'a [(String, ImportMap)],
    pub(super) ts_re_export_pairs: &'a [(String, Vec<zzop_core::ReExport>)],
    pub(super) ts_dynamic_import_pairs: &'a [(String, Vec<String>)],
}

pub(super) struct Finalized {
    /// Embedded whole rather than re-listed field by field: `metrics::compute` already names its own
    /// result, and a second copy of those eight names here would be a second owner of that shape.
    pub(super) metrics: metrics::MetricsResult,
    pub(super) config_warnings: Vec<String>,
    pub(super) rule_timings: Option<Vec<zzop_core::dsl::RuleTiming>>,
    pub(super) ir: zzop_core::CommonIr,
    pub(super) coverage: crate::CoverageCensus,
    pub(super) git_window: Option<crate::GitWindow>,
}

/// Runs every post-findings phase in order and returns their combined result. `warnings` is threaded as
/// `&mut` rather than returned because two different phases append to it and the parent's channel is
/// already open — the same convention every other producer in `assemble` follows.
pub(super) fn run(input: FinalizeInputs<'_>, warnings: &mut Vec<String>) -> Finalized {
    let FinalizeInputs {
        root,
        config,
        nodes,
        dep,
        cycles,
        commits,
        git_active,
        findings,
        mut rule_time,
        file_count,
        all_symbols,
        loc_by_path,
        io,
        parser_dispatched,
        degraded,
        ts_import_pairs,
        ts_re_export_pairs,
        ts_dynamic_import_pairs,
    } = input;

    let metrics_result = metrics::compute(
        config,
        nodes,
        &dep,
        &cycles,
        &commits,
        git_active,
        findings,
        &mut rule_time,
    );

    let diagnostics_report =
        run_diagnostics(file_count, &dep, &all_symbols, &commits, config, git_active);
    warnings.extend(diagnostics_report.warnings);
    let config_warnings = diagnostics_report.config_warnings;

    warnings.extend(diagnose::empty_root_warning(file_count, root));

    let rule_timings = config
        .profile_rules
        .then(|| crate::analyze::sort_rule_timings(rule_time));

    let (ir, coverage) = fuse::ir_and_census(fuse::FuseInputs {
        config,
        dep,
        all_symbols,
        loc_by_path,
        io,
        file_count,
        parser_dispatched,
        degraded,
        ts_import_pairs,
        ts_re_export_pairs,
        ts_dynamic_import_pairs,
    });

    let git_window = metrics::git_window(config, git_active);

    Finalized {
        metrics: metrics_result,
        config_warnings,
        rule_timings,
        ir,
        coverage,
        git_window,
    }
}
