//! Pin for `cross_summary`'s centralized source-mode exclusivity. Both current hosts guard
//! paths/configPath exclusivity at their own boundary (MCP `tools.rs`, CLI argv dispatch), so this
//! error is a safety net for FUTURE hosts — the pin exists so relocating or deleting the check is a
//! visible decision, not silent drift back into "config wins, paths ignored".

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
