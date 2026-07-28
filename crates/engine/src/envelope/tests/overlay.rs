//! Mode B `apply_adapter_overlays` — the APPLIED-vs-DECLARED contract of its returned
//! [`crate::envelope::OverlayApplication`]. The end-to-end effect of `entry_paths` on `dead-candidates`
//! is covered by `tests/analyze_adapter_overlay.rs`; these tests pin the source of truth itself, because
//! the defect they seal (a REJECTED overlay's `is_entry` still exempting its file) was invisible
//! end-to-end — it read as one fewer dead-candidate finding, not as an error.

use crate::envelope::apply_adapter_overlays;

use super::{envelope, projection};

/// A projection whose only declared fact is `is_entry: true` — the shape an adapter uses to say
/// "framework-loaded, not import-reached" (a SvelteKit `hooks.*`, a `.vue` route).
fn entry_projection(path: &str) -> zzop_core::FileProjection {
    let mut p = projection(path, 3);
    p.is_entry = true;
    p
}

#[test]
fn an_applied_overlays_is_entry_path_lands_in_entry_paths_and_covered_paths() {
    let overlays = vec![envelope(vec![
        entry_projection("src/hooks.server.ts"),
        projection("src/plain.svelte", 2),
    ])];
    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    let applied = apply_adapter_overlays(&mut artifacts, &overlays, "test", &mut warnings);
    assert!(applied.entry_paths.contains("src/hooks.server.ts"));
    assert!(applied.covered_paths.contains("src/hooks.server.ts"));
    // A fact-less, non-entry projection is neither covered nor exempt (G8b).
    assert!(!applied.entry_paths.contains("src/plain.svelte"));
    assert!(!applied.covered_paths.contains("src/plain.svelte"));
}

/// THE SEAL: an overlay that fails `validate_envelope` contributes NOTHING — not even its `is_entry`
/// declaration. Reading `EngineConfig::adapter_overlays` directly (what `assemble::rules` used to do)
/// honored this rejected overlay's `is_entry`, leaving a dead file exempt from `dead-candidates` forever.
#[test]
fn a_rejected_overlay_contributes_no_entry_paths() {
    let mut bad = envelope(vec![entry_projection("src/hooks.server.ts")]);
    bad.format = "not-a-normalized-ast".to_string();
    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    let applied = apply_adapter_overlays(&mut artifacts, &[bad], "test", &mut warnings);
    assert!(applied.entry_paths.is_empty());
    assert!(applied.covered_paths.is_empty());
    assert_eq!(
        warnings.len(),
        1,
        "the rejection is disclosed: {warnings:?}"
    );
    assert!(warnings[0].contains("skipped"));
}

/// A rejected overlay must not poison an ACCEPTED sibling in the same run — the skip is per-overlay.
#[test]
fn a_rejected_overlay_does_not_suppress_an_accepted_siblings_entry_path() {
    let mut bad = envelope(vec![entry_projection("src/rejected-entry.ts")]);
    bad.format = "not-a-normalized-ast".to_string();
    bad.parser = "bad-parser/1".to_string();
    let good = envelope(vec![entry_projection("src/good-entry.ts")]);
    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    let applied = apply_adapter_overlays(&mut artifacts, &[bad, good], "test", &mut warnings);
    assert_eq!(
        applied
            .entry_paths
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["src/good-entry.ts"]
    );
}
