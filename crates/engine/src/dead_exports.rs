//! Wires `zzop_rules_graph::find_dead_exports` into the whole-graph assembly pass
//! (`analyze::assemble`), gated behind the native analysis id `"unimported-export"` — the symbol-granularity
//! companion to the file-level `"dead-candidates"` analysis. See `zzop_rules_graph::dead_exports`'s
//! module doc for what counts as a "use" and which files/exports are exempted.
//!
//! `FileArtifact` carries `symbols`/`imports`/`used_names` but not re-exports, dynamic imports, or
//! local export aliases, all three needed for complete coverage (barrel chains, entry-re-export live
//! roots, dynamic-import wildcarding, `export { X as Y }` renames). So this function runs a second,
//! uncached pass: when `"unimported-export"` is enabled, it re-reads and re-parses every dispatched
//! TypeScript file directly off disk rather than extending the cached fused pass — it never consults
//! `zzop_cache::AnalysisCache`.
//!
//! **What that costs, stated plainly**: ONE `parse_module` call per dispatched TypeScript file — but
//! the cost is still paid in FULL on every run, cache hit or not, since this pass never consults the
//! cache: a 100%-cache-hit run still re-reads and re-parses the whole TypeScript tree. It used to be
//! THREE parses per file (`parse_re_exports`, `parse_dynamic_imports` and a since-deleted standalone
//! export-alias entrypoint, each parsing the same `&str` independently); `parse_dead_export_facts` now
//! parses once and runs those same three walks over ONE `Module`, returning the three fact lists
//! together. That bundling is possible precisely because no swc type crosses the crate boundary — the
//! bundle is `Vec<ReExport>` + `Vec<String>` + `Vec<(String, String)>`, exactly what the three calls
//! already returned, so `parse_module` stays `pub(crate)` in `zzop-parser-typescript`. The two
//! individual entrypoints that still have other callers remain; the bundle composes their walks rather
//! than restating them, so the two paths cannot answer differently (pinned in that module's own tests).
//!
//! The algorithm and Finding-shaping live in `zzop-rules-graph`; this module keeps only the filesystem +
//! parser-crate orchestration that rule crates (which depend on `zzop-core` only) deliberately stay
//! free of.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use zzop_core::{Finding, ImportMap, SourceSymbol};
use zzop_parser_typescript::{TsconfigPaths, WorkspacePkg};
use zzop_rules_graph::{DeadExportCandidate, DeadExportInputFile};

/// True for the extensions the dispatch table routes to TypeScript. The whole-tree second passes here and
/// in `analyze`'s call-graph scan re-read + re-parse each `ts_paths` member AS TypeScript; a NON-TS
/// dep-graph participant a Mode B overlay added (e.g. a `.svelte` file whose imports were projected) must
/// be skipped — its dep-graph facts already reached `build_dep` via its projection, and parsing its raw
/// non-TS text as TypeScript would be garbage (and could inject spurious call edges). Extension-based
/// rather than threading the dispatch CONFIG (these passes only ever see TS-or-overlay paths, so a
/// tree's `glob_overrides` are deliberately not consulted) — but T1 on the dispatch TABLE, which is
/// the whole definition of "the dispatch table routes it to TypeScript". It used to restate that
/// table's TypeScript arm as its own `matches!`: two spellings of one fact, unpinned, in one crate.
///
/// Note what stays hand-kept and why: `mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS` and
/// `http_scan::WRITE_SITE_COVERED_EXTENSIONS` live in rule crates that depend on `zzop_core` only, so
/// they cannot reach this table at all — those two are T2, pinned by `call_graph_covered_extensions_pin`
/// below, which is also where the reverse-direction hand-copy of this arm now lives.
pub(crate) fn is_ts_source_ext(rel: &str) -> bool {
    matches!(
        crate::dispatch::dispatch_by_extension(rel),
        Some(crate::dispatch::Language::TypeScript)
    )
}

/// One file's name evidence for `unimported-export`. Two sets, one carrier: they are collected together,
/// travel together, and are consumed by exactly one rule — and pairing them keeps the difference
/// between them impossible to miss. `used` is FLAT and position-blind (an identifier occurs
/// somewhere in the file); `signature` is position-AWARE (a name occurs in some exported
/// declaration's public signature). The public-signature exemption needs exactly that difference,
/// which is why `used` alone could never express it.
#[derive(Debug, Clone, Default)]
pub(crate) struct DeadExportNames {
    pub(crate) used: Vec<String>,
    /// TypeScript-only; empty elsewhere, which simply yields no exemptions.
    pub(crate) signature: Vec<String>,
}

/// Runs the whole-tree dead-export computation and converts each result into a `Finding` at its symbol's
/// declaration line. Returns an empty `Vec` immediately when there are no TypeScript-dispatched files.
///
/// `workspace_pkgs`/`tsconfigs` make the resolver closure workspace-alias- and tsconfig-paths-aware: a
/// symbol exported from package A and consumed only via `import ... from '@scope/pkg-a'` (or a
/// `compilerOptions.paths`-mapped specifier) in package B must resolve back to A's file, or the export
/// looks dead even though it's used.
/// `prescan_import_pairs`: import bindings read out of files no structural parser frontend claims — a
/// `<script>` block in a `.vue`/`.svelte`/`.md`, bare top-level ESM in a `.mdx`, or an Astro `---`
/// fence (`crate::analyze::assemble::prescan::collect_prescan_import_pairs`'s output, which dispatches
/// on extension; nothing here varies per filetype). Each is fed to `zzop_rules_graph::find_dead_exports`
/// as an extra SOURCE-ONLY `DeadExportInputFile`: empty `exports`, so it can never itself be flagged
/// dead, and only its `imports` count, marking whichever `.ts` export it names as imported. It is never
/// added to `ts_paths` — invisible to every OTHER pass this function's `files` loop feeds, exactly the
/// "no new dep-graph node" pin `dep_graph::merge_prescan_fan_in`'s doc explains for `dead-candidates`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dead_export_findings(
    root: &Path,
    ts_paths: &HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    all_symbols: &[SourceSymbol],
    dead_export_names_by_file: &HashMap<String, DeadExportNames>,
    workspace_pkgs: &HashMap<String, WorkspacePkg>,
    tsconfigs: &std::collections::BTreeMap<String, TsconfigPaths>,
    prescan_import_pairs: &[(String, ImportMap)],
    auto_import_names: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    generated_file_markers: &[&str],
) -> Vec<Finding> {
    if ts_paths.is_empty() {
        return Vec::new();
    }

    let imports_by_file: HashMap<&String, &ImportMap> =
        ts_import_pairs.iter().map(|(rel, m)| (rel, m)).collect();

    let mut symbols_by_file: HashMap<&str, Vec<&SourceSymbol>> = HashMap::new();
    let mut symbol_lines: HashMap<(&str, &str), u32> = HashMap::new();
    for s in all_symbols {
        symbols_by_file.entry(s.file.as_str()).or_default().push(s);
        symbol_lines.insert((s.file.as_str(), s.name.as_str()), s.line);
    }

    let mut files: Vec<DeadExportInputFile> = Vec::with_capacity(ts_paths.len());
    let mut rels: Vec<&String> = ts_paths.iter().collect();
    rels.sort(); // deterministic `files` order; free next to the per-entry read+parse below
    for rel in rels {
        if !is_ts_source_ext(rel) {
            continue; // non-TS overlay participant (e.g. .svelte) — not re-parseable as TypeScript
        }
        // Re-exports, dynamic imports and local `export { X as Y }` renames in one parse — all three
        // are read off disk rather than from the cache for the same reason: `FileArtifact` does not
        // carry them, and only this rule needs them.
        let (facts, is_generated) = match std::fs::read(root.join(rel)) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let banner = crate::generated_banner::has_generated_banner;
                (
                    zzop_parser_typescript::parse_dead_export_facts(rel, &text),
                    banner(rel, &text, generated_file_markers),
                )
            }
            // Unreadable (deleted/permission race) — treat as no re-exports/dynamic-imports/
            // aliases rather than failing the whole analysis.
            Err(_) => (zzop_parser_typescript::DeadExportFacts::default(), false),
        };
        let exports: Vec<DeadExportCandidate> = symbols_by_file
            .get(rel.as_str())
            .into_iter()
            .flatten()
            .filter(|s| s.exported)
            .map(|s| DeadExportCandidate {
                name: s.name.clone(),
                kind: s.kind,
                is_default: s.is_default,
            })
            .collect();
        let names = dead_export_names_by_file.get(rel);
        let used_names: HashSet<String> = names
            .map(|n| n.used.iter().cloned().collect())
            .unwrap_or_default();
        let exported_signature_names: HashSet<String> = names
            .map(|n| n.signature.iter().cloned().collect())
            .unwrap_or_default();
        files.push(DeadExportInputFile {
            file: rel.clone(),
            exports,
            imports: imports_by_file
                .get(rel)
                .cloned()
                .cloned()
                .unwrap_or_default(),
            re_exports: facts.re_exports,
            dynamic_imports: facts.dynamic_imports,
            used_names,
            exported_signature_names,
            export_aliases: facts.export_aliases,
            is_generated,
            // Bare-identifier references a build-time auto-import table resolved, which no import
            // statement records — see `analyze::assemble::nuxt_auto_import`, which owns the anchor
            // (`nuxt.config.*`), the directory set and the per-app scope wall. Empty map on every tree
            // without such a config, so every non-Nuxt tree keeps its exact previous numbers.
            auto_import_referenced_names: auto_import_names
                .get(rel)
                .map(|n| n.iter().cloned().collect())
                .unwrap_or_default(),
        });
    }

    // Pre-scan source-only contributions — see this function's own doc for `prescan_import_pairs`.
    // Empty `exports`/`re_exports`/`dynamic_imports`/`used_names`: these entries exist purely to feed
    // `find_dead_exports`' first (import-collecting) loop; its second (dead-checking) loop iterates each
    // file's `exports`, which is empty here, so a pre-scan entry can never surface as a dead-export
    // candidate.
    for (rel, imports) in prescan_import_pairs {
        files.push(DeadExportInputFile {
            file: rel.clone(),
            exports: Vec::new(),
            imports: imports.clone(),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            used_names: HashSet::new(),
            exported_signature_names: HashSet::new(),
            export_aliases: Vec::new(),
            is_generated: false,
            auto_import_referenced_names: HashSet::new(),
        });
    }

    let dead = zzop_rules_graph::find_dead_exports(&files, |specifier, from_file| {
        zzop_parser_typescript::resolve_file_with_workspace(
            specifier,
            from_file,
            ts_paths,
            workspace_pkgs,
            tsconfigs,
        )
    });

    zzop_rules_graph::dead_export_findings(dead, &symbol_lines)
}
#[cfg(test)]
#[path = "dead_exports/covered_extensions_pin.rs"]
mod call_graph_covered_extensions_pin;
