//! Assemble-time disk reads for references the cached per-file pass cannot see: the IMPORT PRE-SCAN
//! (below) and, riding the same read, the NUXT AUTO-IMPORT scan ([`super::nuxt_auto_import`], whose
//! reference roster is this pass's own file text).
//!
//! Import pre-scan wiring — the assemble-time (uncached) bridge between
//! `zzop_parser_typescript::extract_prescan_imports` and this pass's two consumers:
//! `dep_graph::merge_prescan_fan_in` (bumps a `.ts` target's fan-in for `dead-candidates`, source-only —
//! see that function's doc for why the pre-scanned file itself never becomes a dep-graph node) and
//! `crate::dead_exports::dead_export_findings`'s `prescan_import_pairs` parameter (marks the target's
//! exports as imported for `unimported-export`).
//!
//! The three dialect families this covers keep their imports in three different places (a `<script>`
//! block, bare top-level ESM, an Astro `---` fence); WHICH reader runs is the parser crate's question,
//! answered by `zzop_parser_typescript::PRESCAN_IMPORT_HOSTS`, and nothing here varies per filetype.
//!
//! Runs off disk at ASSEMBLE time, exactly like `dead_exports.rs`'s own re-read/re-parse step — this
//! never extends the cached fused-pipeline `FileArtifact`/`FileIrSlice` projection for the pre-scanned
//! file itself, so the win adds no cached FIELD and moves no `CACHE_SCHEMA_VERSION`. That is the cache's
//! SHAPE, not its invalidation: every `.rs` here is inside `FP_ENGINE`, which suffixes every arm of
//! `cache::parser_fingerprint`, so editing this file wipes every language lane. Accepted
//! over-invalidation — see `crates/engine/build.rs`.

use zzop_core::ImportMap;

/// What this phase produces. `import_pairs` is what a pre-scanned file IMPORTS (resolvable
/// specifiers); `auto_import_refs` is who is reached by a bare NAME with no specifier at all — a
/// different question, answered partly from the same bytes, which is why the two ride one phase. See
/// [`super::nuxt_auto_import`] for the second one's measurement and scope wall.
pub(super) struct PrescanResult {
    pub(super) import_pairs: Vec<(String, ImportMap)>,
    pub(super) auto_import_refs: super::nuxt_auto_import::NuxtAutoImportRefs,
}

/// Reads every `prescan_rels` file off disk and extracts its import bindings via
/// `zzop_parser_typescript::extract_prescan_imports`, which dispatches on the file's extension. A file
/// whose reader finds nothing contributes an empty `ImportMap` and is dropped (nothing to feed
/// downstream); an unreadable path (deleted/permission race) is skipped rather than failing the whole
/// analysis — same graceful-degrade convention `dead_exports.rs`'s own disk re-read uses.
///
/// The SAME read also yields an identifier-token roster per file, for the pre-scan hosts
/// [`super::nuxt_auto_import::is_token_roster_host`] admits (`.vue` only) that sit inside a Nuxt app
/// dir and outside its `public/` ([`super::nuxt_auto_import::referrer_app_dir`]) — the bytes are in
/// hand exactly once, and a second pass over a thousand `.vue` files to ask a cheaper question than the
/// first pass asked would be pure waste. On a tree with no `nuxt.config.*` there are no app dirs, so no
/// roster is built and [`super::nuxt_auto_import::scan`] returns empty without reading anything at all.
pub(super) fn collect_prescan(
    root: &std::path::Path,
    prescan_rels: &[String],
    ts_paths: &std::collections::HashSet<String>,
    ts_used_names: &std::collections::HashMap<String, crate::dead_exports::DeadExportNames>,
) -> PrescanResult {
    let app_dirs = super::nuxt_auto_import::app_dirs(ts_paths);
    let mut import_pairs = Vec::new();
    let mut rosters: Vec<(String, std::collections::BTreeSet<String>)> = Vec::new();
    for rel in prescan_rels {
        // `read_for_parse`, not `fs::read`: this list is `prescan_rels`, built from dispatch-`None`
        // files, which is the one population the recursion gate used to skip on purpose ("nothing will
        // parse it") while this very line handed it to swc. See that function's doc for the
        // reproduction (review ledger V127).
        let Some(text) = crate::analyze::read_for_parse(root, rel) else {
            continue;
        };
        let imports = zzop_parser_typescript::extract_prescan_imports(rel, &text);
        if !imports.is_empty() {
            import_pairs.push((rel.clone(), imports));
        }
        if super::nuxt_auto_import::is_token_roster_host(rel)
            && super::nuxt_auto_import::referrer_app_dir(rel, &app_dirs).is_some()
        {
            rosters.push((
                rel.clone(),
                super::nuxt_auto_import::identifier_tokens(&text),
            ));
        }
    }
    let auto_import_refs =
        super::nuxt_auto_import::scan(root, ts_paths, &app_dirs, ts_used_names, &rosters);
    PrescanResult {
        import_pairs,
        auto_import_refs,
    }
}
