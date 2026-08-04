//! Pins for `cross_summary`'s centralized source-mode judgments — BOTH sources and NEITHER. Both
//! current hosts guard paths/configPath exclusivity at their own boundary (MCP `tools.rs`, CLI argv
//! dispatch), so these errors are a safety net for FUTURE hosts — the pins exist so relocating or
//! deleting a check is a visible decision, not silent drift back into "config wins, paths ignored".

use crate::output::FindingFilters;

#[test]
fn cross_summary_rejects_paths_and_config_path_together() {
    let err = super::cross::cross_summary(
        &["a".to_string(), "b".to_string()],
        Some("zzop.config.jsonc"),
        &FindingFilters::from_args(None).expect("no-args filters"),
    )
    .unwrap_err();
    assert!(
        err.contains("not both"),
        "both sources must be an explicit error, never a silently-narrowed join: {err}"
    );
}

/// NEITHER source: the join lane's no-source refusal must name EVERY source it accepts.
///
/// Until 2026-08-02 it named only one. A call with no `paths` and no config fell through to
/// `zero_config_trees`, whose sentence is about paths mode alone — "needs at least 2 paths (e.g. the
/// frontend and the backend)" — so a caller holding a config file was told to go find a second
/// directory instead. The single-tree-tolerant sibling (`resolve_trees_request`, behind the endpoint
/// and file queries) had had an explicit neither-branch naming all three of ITS sources the whole
/// time; this lane simply lacked the branch.
///
/// The asymmetry the assertions below seal: this sentence names TWO sources, not the sibling's three.
/// One tree root is a legal source THERE and refused HERE (a join needs two sides), so a future edit
/// that "harmonizes" the two messages by copying the sibling's would advertise a source this function
/// rejects one branch later.
#[test]
fn cross_summary_no_source_at_all_names_the_config_file_too_not_just_paths() {
    let err = super::cross::cross_summary(
        &[],
        None,
        &FindingFilters::from_args(None).expect("no-args filters"),
    )
    .unwrap_err();
    assert!(
        err.contains("2+ tree roots"),
        "the no-source refusal must name paths mode: {err}"
    );
    assert!(
        err.contains("config file") && err.contains("`trees`"),
        "a config file whose `trees` define the join is equally valid here and the refusal has to say \
         so — naming only paths sends a caller who already has one off to find a second directory: \
         {err}"
    );
    assert!(
        !err.contains("pass one tree root"),
        "the join lane must NOT offer a single tree root the way the single-tree-tolerant sibling \
         does — that source is refused here: {err}"
    );
    assert!(
        err.contains("the cross-layer join"),
        "shared-helper errors name the calling operation, never a hardcoded sibling: {err}"
    );
}
