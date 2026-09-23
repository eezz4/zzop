//! Applying a request's DECLARED knobs onto the engine types that own them.
//!
//! Split out of `config.rs` on 2026-07-27, when opening the parser-routing surface pushed that file past
//! the repo's line cap. The seam is a real one rather than a size-driven cut: everything here answers the
//! same question — "the author declared X; which engine-side owner does X land on, and what does an
//! undeclared or malformed X mean?" — while the rest of `config.rs` assembles packs and rules.
//!
//! The rule these follow splits in two, and reading it as one unconditional rule is wrong (corrected
//! 2026-08-14 — it was written as "a declared value is applied WHOLE, the empty declaration included"
//! with no qualifier, which is a true sentence about one of the two kinds below and a false one about
//! the other):
//!
//! - REPLACEMENT knobs — everything sourced from `req.vocabulary` — ARE applied whole, the empty
//!   declaration included. Letting an owner type's `Default` come back for an empty declaration is the
//!   built-in-behind-the-author's-back that the 2026-07-27 vocabulary arc removed everywhere else.
//! - `parsers.globOverrides` is NOT one of them: it PUSHES onto `DispatchConfig::glob_overrides`, an
//!   ADDITIVE tier consulted ahead of the extension map, which every unmatched path still falls through
//!   to (`dispatch::dispatch` -> `dispatch_by_extension`). So an empty declaration here replaces nothing
//!   and the extension map keeps answering — correct for a routing override, and the exact opposite of
//!   what the whole-replacement rule predicts. Read unconditionally, that rule says declaring
//!   `parsers: {}` blanks the extension map. It does not.

use zzop_engine::EngineConfig;

use crate::request::AnalyzeRequest;

/// Lands every declared vocabulary/routing knob on its owner, pushing a warning for anything the author
/// spelled that this build cannot honor.
pub(crate) fn apply_declared(
    config: &mut EngineConfig,
    req: &AnalyzeRequest,
    warnings: &mut Vec<String>,
) {
    // Declared convention vocabulary. `skipDirs` is split off into `dispatch`, which already owned the
    // walker's skip list — one list, one owner, so a declared value and a default can never both be live.
    // Every other key stays on `vocabulary` and is read at its use site.
    //
    // The request's list is applied WHOLE, empty included (2026-07-27). It used to be applied only when
    // non-empty, which let `DispatchConfig::default()` come back for an empty declaration — the same
    // built-in-behind-the-author's-back this batch removed everywhere else. `DispatchConfig::default()`
    // survives for the Rust library embedder, who has no config file to declare from; a request that came
    // from one carries what the config said, and `zzop init` writes zzop's own list into it.
    config.vocabulary = req.vocabulary.clone();
    config.dispatch.skip_dirs = req.vocabulary.skip_dirs.clone();
    // Same split as `skipDirs`, for the same reason: these three axes are read through types that owned
    // the list before it was declarable (`IoOptions`, `ScoresConfig`), so the declared value is written
    // ONTO that owner rather than left on `vocabulary` for a second reader to find. Assigned WHOLE,
    // empty included — an empty declaration must not let the owner's `Default` come back, which is the
    // built-in-behind-the-author's-back this arc removed everywhere else.
    // Parser routing. An unknown language name is a config-authoring mistake, so it lands in
    // `configWarnings` naming the accepted spellings — never a silent drop (the author would see the
    // file still analyzed by extension and conclude the override worked) and never a hard failure (the
    // rest of the run has honest answers). Same verdict shape as an unreadable overlay.
    for entry in &req.parsers.glob_overrides {
        match zzop_engine::Language::from_wire(&entry.language) {
            Some(lang) => config
                .dispatch
                .glob_overrides
                .push((entry.glob.clone(), lang)),
            None => warnings.push(format!(
                "parsers.globOverrides entry for '{}' names language \"{}\", which this build does not \
                 have — accepted: {}",
                entry.glob,
                entry.language,
                zzop_engine::Language::WIRE_NAMES
                    .iter()
                    .map(|l| l.as_wire())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    // Structural-score policy. A BOOL, not a list, so the whole-replacement rule above has nothing to
    // say about it: absent means `false` means "count every file", which is the population every score
    // has always used. The engine turns the `true` into a `zzop_metrics::PopulationFilter` at compute
    // time, because that is the layer that can also read this tree's `vocabulary.extraTestPathPatterns`.
    config.scores_exclude_test_files = req.scores.exclude_test_files_from_file_metrics;
    // The retired spelling is REPORTED, never honored. Serde drops an unknown field without a word, so
    // a config upgraded across this rename would otherwise change its own `pain` with nothing said —
    // which is a worse failure than the naming gap the rename repairs (review ledger V182).
    //
    // This is the SECOND of the two moved-key outcomes `VERSIONING.md`s config-keys row names —
    // reported at exit 0 rather than refused at exit 1 — and which one a key gets follows what its
    // stale value would silently do: mis-key the cross-tree join (refuse) or move a score (report).
    // That row stated only the refusing half until 2026-09-23, so the two halves of one release said
    // opposite things about the same key class. If a future rename lands here, say which outcome it
    // takes and why, in BOTH places.
    if req.scores.exclude_test_files_from_population.is_some() {
        warnings.push(
            "scores.excludeTestFilesFromPopulation was RENAMED to \
             scores.excludeTestFilesFromFileMetrics and this run IGNORED it — the structural scores \
             counted every file, including tests. The old name claimed more than the key ever did: it \
             narrows the metrics whose subject is a FILE, while the four keyed on a directory rollup \
             (cohesion, sdp, mainSequence, modularity) and the criticalTop/topRecommendation rankings \
             always counted the whole tree and still do. Rename the key to restore the behaviour you \
             had."
                .to_string(),
        );
    }

    config.io.router_names = req.vocabulary.router_names.clone();
    config.scores_config.hierarchy_shared_dirs = req
        .vocabulary
        .hierarchy_shared_dirs
        .iter()
        .cloned()
        .collect();
    config.scores_config.feature_sliced_design =
        zzop_metrics::FeatureSlicedDesignMatcher::new(zzop_metrics::FeatureSlicedDesignConfig {
            slice_containers: req
                .vocabulary
                .feature_sliced_design
                .slice_containers
                .clone(),
            entry: req.vocabulary.feature_sliced_design.entry.clone(),
            shared: req.vocabulary.feature_sliced_design.shared.clone(),
            base_dirs: req.vocabulary.feature_sliced_design.base_dirs.clone(),
        });
}
