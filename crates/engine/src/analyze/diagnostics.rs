//! Capability self-reports / warnings / git collection — the honest-degrade surface `assemble` folds
//! into `AnalyzeOutput::warnings`: git collection (or its absence), the coverage-gap diagnostics report,
//! zero-DSL-packs and minified-file-skip self-reports, and unknown-disabled-rule-id detection.
//!
//! Module root only re-exports; the substance lives in the submodules. `minified_files_warning` stays
//! here because the policy census (`scripts/policy-census.txt`) pins the [`SAMPLE`] constant to this
//! path — which is also why `SAMPLE` is declared at module scope and shared with the sibling file-axis
//! report rather than copied: a second declaration would be a second censused name for one policy.

mod capability;
mod config_filters;
mod coverage_report;
mod degraded_files;
mod git_collect;
mod pack_scope;

#[cfg(test)]
mod unmatched_suppression_tests;
#[cfg(test)]
mod unparsed_extension_tests;

pub(super) use capability::{git_not_requested_warning, unparsed_extension_warning};
pub(crate) use capability::{uncompilable_rule_warnings, zero_packs_warning};
pub(crate) use config_filters::{
    global_exclude_diagnostics, unmatched_global_exclude_warnings, unmatched_suppression_warnings,
};
pub(crate) use coverage_report::{rule_overrides_applied, run_diagnostics};
pub(super) use degraded_files::degraded_files_warning;
pub(super) use git_collect::collect_git;
pub(crate) use git_collect::GitCache;
pub(crate) use pack_scope::{
    compute_dsl_scope, compute_dsl_scope_filtered, pack_scope_warnings, DslScope,
};

/// Example paths a FILE-AXIS aggregate self-report names inline before collapsing the rest to a
/// `+N more` count. Shared by every report whose subject is "these individual files fell out of the
/// flow" — [`minified_files_warning`] here and `pack_scope::vetoed_files` — because they are read in the
/// same sitting by the same reader, and a run that printed 3 examples for one and 8 for the other would
/// be inviting a comparison of the sample sizes rather than of the counts. Three is enough to recognise
/// a pattern ("oh, these are all fixtures") and small enough that the line stays one line; the exact
/// list is never the deliverable, since the counts and the lever are.
pub(crate) const SAMPLE: usize = 3;

/// Capability self-report: how many files this run classified minified/generated and were therefore
/// skipped for every DSL rule-pack matcher type (distinct from `degraded`, which still runs line-scan
/// rules). One aggregate entry, never one per file. `sorted_rels` must already be sorted. Returns `None`
/// when nothing was skipped this way.
///
/// ## Only files a rule would actually have run on
/// `in_scope_rels` is [`compute_dsl_scope`]'s per-file union — every analyzed rel matching >=1 loaded
/// rule's `file_pattern` — and a rel outside it is dropped here BEFORE counting. The minified/generated
/// classification is a property of the file's TEXT (`zzop_core::dsl::has_minified_line_shape`), computed
/// before any pack's `file_pattern` is consulted, so it fires on files that were never DSL candidates to
/// begin with: a field run reported 118 skips whose examples were project-notes `*.md` files and a `*.png`
/// asset. A PNG is not minified source — it is a binary no rule targets, and calling its exclusion a
/// "skip" claims coverage was lost where none existed. This filter makes the number mean what the sentence
/// says: files a DSL rule WOULD have run on, and did not.
///
/// Deliberately keyed off the shared scope census rather than a local re-derivation of "is this a
/// candidate" — a second copy of that predicate would drift from the one `no_applicable_dsl_rule_warning`
/// and `packs_loaded`'s `files_in_scope` already use. With no packs loaded, nothing is in scope and this
/// warning correctly falls silent (`zero_packs_warning` owns that disclosure).
pub(super) fn minified_files_warning(
    sorted_rels: &[String],
    in_scope_rels: &std::collections::BTreeSet<String>,
) -> Option<String> {
    let skipped: Vec<&str> = sorted_rels
        .iter()
        .filter(|rel| in_scope_rels.contains(*rel))
        .map(String::as_str)
        .collect();
    if skipped.is_empty() {
        return None;
    }
    let mut sample_str = skipped
        .iter()
        .take(SAMPLE)
        .copied()
        .collect::<Vec<&str>>()
        .join(", ");
    if skipped.len() > SAMPLE {
        sample_str.push_str(&format!(", +{} more", skipped.len() - SAMPLE));
    }
    Some(format!(
        "{} file(s) a DSL rule-pack rule targets have MINIFIED LINE SHAPE and were skipped for ALL DSL rule-pack rules (5000+ byte single lines, or long lines dominating half the file's bytes; native structural analyses still cover them): {sample_str}. This is a measurement of line shape ONLY -- it does not decide whether a file is machine-generated, and a generated file with ordinary line lengths is not skipped by it. Files no loaded rule's `file_pattern` targets (docs, data, images, ...) are not counted here — they were never DSL candidates, so nothing was skipped for them.",
        skipped.len()
    ))
}
