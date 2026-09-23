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
    pack_scope_warnings, suppressed_findings_warning, uncompilable_rule_warnings,
    unmatched_suppression_warnings, unparsed_extension_warning, DslScope,
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
    /// Every suppression marker in the tree — see `suppressed_findings_warning`.
    pub(super) suppress_markers: &'a [zzop_core::dsl::SuppressMarkerSite],
    /// Every degraded file with its cause — the substrate for the one self-report that can tell an
    /// oversized file from an unreadable one from a parse failure.
    pub(super) degraded: &'a [DegradedFile],
    pub(super) unparsed_extensions: &'a BTreeMap<String, (usize, Vec<String>)>,
    pub(super) ts_paths: &'a HashSet<String>,
    pub(super) java_rels: &'a [String],
    pub(super) csharp_rels: &'a [String],
    pub(super) package_import_files: &'a BTreeMap<String, BTreeSet<String>>,
    pub(super) loc_by_path: &'a HashMap<String, u32>,
    /// The two projection channels this module's own measurement needs and no other field carries.
    /// Together with `io_provides`/`io_consumes` (passed to [`sweep`] directly) and `degraded` above,
    /// they are the whole substrate of S17's structural half — see
    /// `zzop_engine::zero_extraction::structural_by_ext` for why all three channels are in it.
    pub(super) all_symbols: &'a [zzop_core::ir::SourceSymbol],
    pub(super) dep: &'a HashMap<String, Vec<String>>,
    /// What each accepted adapter overlay contributed, by `parser` id — the provenance the merge is
    /// about to make unrecoverable. See `diagnostics::overlay_provenance`.
    pub(super) overlay_io: &'a BTreeMap<String, crate::envelope::OverlayIoCounts>,
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
    // The lexically-visible route-registration count, set only when the provide-side trio measured it
    // (S2's precondition). Rides out of this phase because its second reader is the run-wide
    // provide-blind severity gate, which cannot re-derive the file set without risking a different
    // population than the extractor saw — see `provide_side_warnings`.
    visible_route_registrations_out: &mut Option<usize>,
) -> DslScope {
    let config = input.config;
    // One census, three consumers: both pack warnings below and `packs_loaded`'s `files_in_scope` count.
    let dsl_scope = compute_dsl_scope(&config.packs, input.rels, &config.dispatch);
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
    // Sits with the other config-filter self-reports: the marker is a suppression the AUTHOR wrote, the
    // same axis as an `exclude` entry, and the only one whose effect leaves no number behind.
    warnings.extend(suppressed_findings_warning(input.suppress_markers));
    warnings.extend(global_exclude_diagnostics(config, input.rels));
    warnings.extend(pack_scope_warnings(config, &dsl_scope));
    warnings.extend(uncompilable_rule_warnings(&config.packs)); // dead rule != quiet rule

    // Same subject as the line above — a property of the LOADED pack set, not of this tree — and the
    // same reason it is disclosed rather than fixed by construction: two packs whose rules share a
    // bare id derive the SAME `zzop-<id>-ok` marker, so one vetted suppression comment silently
    // silences both. See `zzop_core::suppress_marker_collisions` for why the marker grammar is not
    // namespaced away instead.
    warnings.extend(zzop_core::suppress_marker_collisions(&config.packs));

    // S17's measured half, computed HERE rather than by the caller: the self-reports are this module's
    // subject, and an orchestrator that assembled this map would be a second owner of what "structural"
    // means. `zzop_engine::zero_extraction::structural_by_ext` owns the definition itself.
    let structural_by_ext = super::helpers::structural_exts(
        input.rels,
        input.all_symbols,
        input.dep,
        io_provides,
        io_consumes,
        input.degraded,
    );
    warnings.extend(super::warnings::framework_silence_warnings(
        input.root,
        io_provides,
        io_consumes,
        input.ts_paths,
        input.java_rels,
        input.csharp_rels,
        input.package_import_files,
        input.loc_by_path,
        &structural_by_ext,
        &config.vocabulary.resolve().fetch_wrapper_export_names,
        &config.rule_config,
        input.overlay_io,
        visible_route_registrations_out,
    ));
    // AFTER the tripwires, deliberately. Those say "zzop cannot see this framework"; this says "and
    // here is what a caller handed it instead". Read in that order the pair is one story; reversed, the
    // provenance line reads as an accusation before anything explains why an adapter was needed.
    warnings.extend(crate::analyze::diagnostics::overlay_provenance_warning(
        input.overlay_io,
        io_provides.iter().filter(|p| p.kind == "http").count(),
    ));

    dsl_scope
}

/// The emptiest self-report there is: this run walked the root and found nothing to analyze.
///
/// `root.is_dir()` gates it so it does not duplicate `analyze_tree`'s more specific "root missing / not
/// a directory" report (`lib.rs`'s `scope_warnings`); an existing-but-empty root gets no such sibling,
/// which is exactly the case this covers. It lives here rather than in the orchestrator for the reason
/// every other line in this module does: a self-report is this module's subject.
pub(super) fn empty_root_warning(file_count: usize, root: &std::path::Path) -> Option<String> {
    (file_count == 0 && root.is_dir()).then(|| {
        "root produced 0 analyzable files — check the path exists and contains supported source files"
            .to_string()
    })
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
