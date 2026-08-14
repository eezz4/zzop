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
