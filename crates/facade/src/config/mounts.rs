//! The deployment-topology mount fold — one function, shared by both request paths so the two wire
//! paths cannot drift on mount ordering.
//!
//! Split out of `config.rs` on 2026-07-27 to stay under the repo's per-file line cap, when the
//! convention-vocabulary assignment block grew that file past it. This is the seam with the fewest ties
//! to the rest: `fold_mounts` reads only its two arguments and is called from two places.

use zzop_engine::MountRule;

use crate::request::MountEntryRequest;

/// Deployment-topology mount fold, shared by BOTH request paths (`build_engine_config` for
/// tree-rooted requests, `analyze_envelope_json` for envelope requests — one fold, so the two wire
/// paths cannot drift): every `mounts[]` entry folds in FIRST, in array order, followed by
/// `mounted_at` as the implicit whole-tree entry (`dir: ""`) LAST. The engine's own
/// `apply_config_mounts` picks the longest matching `dir` on a match and resolves equal-length ties
/// to the first entry — appending `mounted_at` last so an explicit dir entry of equal length wins
/// ties (an explicit `{dir:"", at:"..."}` mount, the one shape that can tie with `mounted_at`'s
/// empty `dir`, is more specific intent than the shorthand and should win). No shape validation
/// happens here (see `AnalyzeRequest::mounted_at`/`mounts`'s docs) — this is a plain, unchecked
/// pass-through.
pub(crate) fn fold_mounts(
    mounts: &[MountEntryRequest],
    mounted_at: Option<&str>,
) -> Vec<MountRule> {
    let mut folded: Vec<MountRule> = mounts
        .iter()
        .map(|m| MountRule {
            dir: m.dir.clone(),
            at: m.at.clone(),
        })
        .collect();
    if let Some(at) = mounted_at {
        folded.push(MountRule {
            dir: String::new(),
            at: at.to_string(),
        });
    }
    folded
}
