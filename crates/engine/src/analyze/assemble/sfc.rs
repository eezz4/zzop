//! `.vue`/`.svelte` SFC `<script>`-block import pre-scan wiring — the assemble-time (uncached) bridge
//! between `zzop_parser_typescript::extract_sfc_script_imports` and this pass's two consumers:
//! `dep_graph::merge_sfc_fan_in` (bumps a `.ts` target's fan-in for `dead-candidates`, source-only — see
//! that function's doc for why the `.vue`/`.svelte` file itself never becomes a dep-graph node) and
//! `crate::dead_exports::dead_export_findings`'s `sfc_import_pairs` parameter (marks the target's exports
//! as imported for `dead-exports`).
//!
//! Runs off disk at ASSEMBLE time, exactly like `dead_exports.rs`'s own re-read/re-parse step — this
//! never extends the cached fused-pipeline `FileArtifact`/`FileIrSlice` projection for the `.vue`/
//! `.svelte` file itself, so the win costs no `PARSER_FINGERPRINT`/`CACHE_SCHEMA_VERSION` bump.

use zzop_core::ImportMap;

/// Reads every `sfc_rels` file off disk and extracts its `<script>`-block import bindings via
/// `zzop_parser_typescript::extract_sfc_script_imports`. A file with no `<script>` block contributes an
/// empty `ImportMap` and is dropped (nothing to feed downstream); an unreadable path (deleted/permission
/// race) is skipped rather than failing the whole analysis — same graceful-degrade convention
/// `dead_exports.rs`'s own disk re-read uses.
pub(super) fn collect_sfc_import_pairs(
    root: &std::path::Path,
    sfc_rels: &[String],
) -> Vec<(String, ImportMap)> {
    let mut pairs = Vec::new();
    for rel in sfc_rels {
        let Ok(bytes) = std::fs::read(root.join(rel)) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let imports = zzop_parser_typescript::extract_sfc_script_imports(rel, &text);
        if !imports.is_empty() {
            pairs.push((rel.clone(), imports));
        }
    }
    pairs
}
