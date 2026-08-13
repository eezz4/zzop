//! Phase 5b: the post-rules **self-report sweep** — every `warnings` entry that can only be written
//! once the whole tree's rules have run, plus the io fold that closes the fact-collection half of
//! `assemble`.
//!
//! # Why this is its own phase rather than a tail of `assemble`
//! It reads nothing the earlier phases produce as a *substrate*; it reads what they produced as a
//! *result*, and asks the one question none of them can answer alone: "what did this run fail to see,
//! and did anyone say so?" Two of the entries here are load-bearing in exactly that way —
//! `minified_files_warning` and `unparsed_extension_warning` are the difference between "0 findings
//! because the code is clean" and "0 findings because nobody looked" — so they belong with each other
//! rather than scattered through the orchestrator.
//!
//! The `dsl_scope` census is computed here because it has three consumers and only one honest
//! computation: the two pack warnings below, and `packs_loaded`'s `files_in_scope` count back in the
//! caller. Computing it twice would let a run report two different scopes for one pack.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use zzop_core::{IoConsume, IoFacts, IoProvide};

use crate::analyze::diagnostics::{
    compute_dsl_scope, degraded_files_warning, global_exclude_diagnostics, minified_files_warning,
    pack_scope_warnings, uncompilable_rule_warnings, unmatched_suppression_warnings,
    unparsed_extension_warning, DslScope,
};
use crate::EngineConfig;

use super::collect::DegradedFile;

/// The substrates this sweep reads. A struct rather than a dozen positional parameters: every field is
/// a borrow of something the caller already owns, and the names are what make the call site readable.
pub(super) struct DiagnoseInput<'a> {
    pub(super) root: &'a std::path::Path,
    pub(super) config: &'a EngineConfig,
    /// Sorted `rel` paths of every analyzed file — the denominator every scope/suppression census below
    /// divides by.
    pub(super) rels: &'a [&'a str],
    pub(super) minified: &'a [String],
    /// Every degraded file with its cause — the substrate for the one self-report that can tell an
    /// oversized file from an unreadable one from a parse failure.
    pub(super) degraded: &'a [DegradedFile],
    pub(super) unparsed_extensions: &'a BTreeMap<String, (usize, Vec<String>)>,
    pub(super) ts_paths: &'a HashSet<String>,
    pub(super) java_rels: &'a [String],
    pub(super) csharp_rels: &'a [String],
    pub(super) package_import_files: &'a BTreeMap<String, BTreeSet<String>>,
    pub(super) loc_by_path: &'a HashMap<String, u32>,
}

/// Appends every post-rules self-report to `warnings`, and returns the pack-scope census the caller
/// still needs for `packs_loaded`.
///
/// `io_provides`/`io_consumes` are taken by reference here because the framework-silence probe reads
/// them; the caller folds them into [`IoFacts`] afterwards via [`fold_io`], which consumes them.
pub(super) fn sweep(
    input: &DiagnoseInput<'_>,
    io_provides: &[IoProvide],
    io_consumes: &[IoConsume],
    warnings: &mut Vec<String>,
) -> DslScope {
    let config = input.config;
    // One census, three consumers: both pack warnings below and `packs_loaded`'s `files_in_scope` count.
    let dsl_scope = compute_dsl_scope(&config.packs, input.rels);
    if let Some(w) = minified_files_warning(input.minified, &dsl_scope.in_scope_rels) {
        warnings.push(w);
    }
    // Sits beside `minified_files_warning` and `unparsed_extension_warning` for this module's own stated
    // reason: it is the third answer to "0 findings because the code is clean, or because nobody looked?"
    // — and the one whose subject zzop actually opened and read.
    if let Some(w) = degraded_files_warning(input.degraded, config) {
        warnings.push(w);
    }
    warnings.extend(unparsed_extension_warning(input.unparsed_extensions));
    warnings.extend(unmatched_suppression_warnings(config, input.rels));
    warnings.extend(global_exclude_diagnostics(config, input.rels));
    warnings.extend(pack_scope_warnings(config, &dsl_scope));
    warnings.extend(uncompilable_rule_warnings(&config.packs)); // dead rule != quiet rule

    // Same subject as the line above — a property of the LOADED pack set, not of this tree — and the
    // same reason it is disclosed rather than fixed by construction: two packs whose rules share a
    // bare id derive the SAME `zzop-<id>-ok` marker, so one vetted suppression comment silently
    // silences both. See `zzop_core::suppress_marker_collisions` for why the marker grammar is not
    // namespaced away instead.
    warnings.extend(zzop_core::suppress_marker_collisions(&config.packs));

    warnings.extend(super::warnings::framework_silence_warnings(
        input.root,
        io_provides,
        io_consumes,
        input.ts_paths,
        input.java_rels,
        input.csharp_rels,
        input.package_import_files,
        input.loc_by_path,
        &config.vocabulary.resolve().fetch_wrapper_export_names,
        &config.rule_config,
    ));

    dsl_scope
}

/// Folds the two io lists into `CommonIr.io`. `None` when BOTH are empty — an absent `io` block says
/// "this tree provides and consumes nothing", which is what an empty pair means; an `IoFacts` holding
/// two empty vectors would say the same thing in more bytes and give the cross-layer join a shape to
/// walk for no reason.
pub(super) fn fold_io(provides: Vec<IoProvide>, consumes: Vec<IoConsume>) -> Option<IoFacts> {
    if provides.is_empty() && consumes.is_empty() {
        None
    } else {
        Some(IoFacts { provides, consumes })
    }
}
