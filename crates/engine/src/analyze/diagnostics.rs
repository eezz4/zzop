//! Capability self-reports / warnings / git collection — the honest-degrade surface `assemble` folds
//! into `AnalyzeOutput::warnings`: git collection (or its absence), the coverage-gap diagnostics report,
//! zero-DSL-packs and minified-file-skip self-reports, and unknown-disabled-rule-id detection.
//!
//! Module root only re-exports; the substance lives in the submodules. `minified_files_warning` stays
//! here because the policy census (`scripts/policy-census.txt`) pins its `SAMPLE` constant to this path.

mod capability;
mod config_filters;
mod coverage_report;
mod git_collect;

#[cfg(test)]
mod unmatched_suppression_tests;
#[cfg(test)]
mod unparsed_extension_tests;

pub(crate) use capability::{
    compute_dsl_scope, no_applicable_dsl_rule_warning, zero_packs_warning,
};
pub(super) use capability::{git_not_requested_warning, unparsed_extension_warning};
pub(crate) use config_filters::{
    unmatched_global_exclude_warnings, unmatched_suppression_warnings,
};
pub(crate) use coverage_report::{rule_overrides_applied, run_diagnostics};
pub(super) use git_collect::collect_git;

/// Capability self-report: how many files this run classified minified/generated and were therefore
/// skipped for every DSL rule-pack matcher type (distinct from `degraded`, which still runs line-scan
/// rules). One aggregate entry, never one per file. `sorted_rels` must already be sorted. Returns `None`
/// when nothing was skipped this way.
pub(super) fn minified_files_warning(sorted_rels: &[String]) -> Option<String> {
    if sorted_rels.is_empty() {
        return None;
    }
    const SAMPLE: usize = 3;
    let sample: Vec<&str> = sorted_rels
        .iter()
        .take(SAMPLE)
        .map(String::as_str)
        .collect();
    let mut sample_str = sample.join(", ");
    if sorted_rels.len() > SAMPLE {
        sample_str.push_str(&format!(", +{} more", sorted_rels.len() - SAMPLE));
    }
    Some(format!(
        "{} minified/generated file(s) skipped for ALL DSL rule-pack rules (long-line-dominated, 5000+ byte single lines, or binary-looking content; native structural analyses still cover them): {sample_str}",
        sorted_rels.len()
    ))
}
