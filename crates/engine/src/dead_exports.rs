//! Wires `zzop_rules_graph::find_dead_exports` into the whole-graph assembly pass
//! (`analyze::assemble`), gated behind the native analysis id `"dead-exports"` — the symbol-granularity
//! companion to the file-level `"dead-candidates"` analysis. See `zzop_rules_graph::dead_exports`'s
//! module doc for what counts as a "use" and which files/exports are exempted.
//!
//! `FileArtifact` carries `symbols`/`imports`/`used_names` but not re-exports or dynamic imports, both
//! needed for complete coverage (barrel chains, entry-re-export live roots, dynamic-import
//! wildcarding). So this function runs a second, uncached pass: when `"dead-exports"` is enabled, it
//! re-reads and re-parses every dispatched TypeScript file directly off disk rather than extending the
//! cached fused pass — it never consults `zzop_cache::AnalysisCache`.
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
/// non-TS text as TypeScript would be garbage (and could inject spurious call edges). Extension-based and
/// duplicated rather than threading the dispatch config: these passes only ever see TS-or-overlay paths.
pub(crate) fn is_ts_source_ext(rel: &str) -> bool {
    matches!(
        Path::new(rel)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        // If you add/remove an extension here, also update the reverse-direction snapshot in
        // `call_graph_covered_extensions_pin` below AND `mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS`
        // — this predicate has no enumerable set to check live, so that pin guards the duplicate by hand.
        Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts")
    )
}

/// Runs the whole-tree dead-export computation and converts each result into a `Finding` at its symbol's
/// declaration line. Returns an empty `Vec` immediately when there are no TypeScript-dispatched files.
///
/// `workspace_pkgs`/`tsconfigs` make the resolver closure workspace-alias- and tsconfig-paths-aware: a
/// symbol exported from package A and consumed only via `import ... from '@scope/pkg-a'` (or a
/// `compilerOptions.paths`-mapped specifier) in package B must resolve back to A's file, or the export
/// looks dead even though it's used.
pub(crate) fn dead_export_findings(
    root: &Path,
    ts_paths: &HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    all_symbols: &[SourceSymbol],
    used_names_by_file: &HashMap<String, Vec<String>>,
    workspace_pkgs: &HashMap<String, WorkspacePkg>,
    tsconfigs: &std::collections::BTreeMap<String, TsconfigPaths>,
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
    for rel in ts_paths {
        if !is_ts_source_ext(rel) {
            continue; // non-TS overlay participant (e.g. .svelte) — not re-parseable as TypeScript
        }
        let (re_exports, dynamic_imports) = match std::fs::read(root.join(rel)) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                (
                    zzop_parser_typescript::parse_re_exports(rel, &text),
                    zzop_parser_typescript::parse_dynamic_imports(rel, &text),
                )
            }
            // Unreadable (deleted/permission race) — treat as no re-exports/dynamic-imports rather
            // than failing the whole analysis.
            Err(_) => (Vec::new(), Vec::new()),
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
        let used_names: HashSet<String> = used_names_by_file
            .get(rel)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        files.push(DeadExportInputFile {
            file: rel.clone(),
            exports,
            imports: imports_by_file
                .get(rel)
                .cloned()
                .cloned()
                .unwrap_or_default(),
            re_exports,
            dynamic_imports,
            used_names,
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

/// T2 policy-value pin (rule-quality.md §6 substitute for a T1 shared symbol): `rules-http`'s
/// `mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS` is a hand-maintained duplicate of
/// [`is_ts_source_ext`]'s accepted extension set, plus `"java"` (that crate depends on `zzop_core`
/// only, so it cannot call this private fn directly — its own doc says as much). Lives here, not in
/// `crates/engine/tests/`, because [`is_ts_source_ext`] is `pub(crate)`: an external integration-test
/// crate cannot see it, only a unit test inside this same module can. If this fails, either
/// `is_ts_source_ext` grew/shrank an extension and the rule's list needs the same edit, or the rule's
/// list drifted on its own — either way, re-justify both sides together.
#[cfg(test)]
mod call_graph_covered_extensions_pin {
    use super::is_ts_source_ext;

    #[test]
    fn call_graph_covered_extensions_equals_is_ts_source_ext_plus_java() {
        let rule_list = zzop_rules_http::mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS;
        assert!(
            rule_list.contains(&"java"),
            "the rule's list must still carry its one deliberate addition beyond \
             is_ts_source_ext, got: {rule_list:?}"
        );
        for ext in rule_list {
            if *ext == "java" {
                continue; // the one deliberate, documented addition beyond is_ts_source_ext
            }
            assert!(
                is_ts_source_ext(&format!("x.{ext}")),
                "rule's CALL_GRAPH_COVERED_EXTENSIONS lists {ext:?}, but is_ts_source_ext does not \
                 accept it — the two hand-kept duplicates have drifted apart"
            );
        }
        // The reverse direction: every extension is_ts_source_ext accepts (enumerated from its own
        // match arm — see that fn's source) must also be in the rule's list.
        for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"] {
            assert!(
                rule_list.contains(&ext),
                "is_ts_source_ext accepts {ext:?}, but rule's CALL_GRAPH_COVERED_EXTENSIONS does \
                 not list it — the two hand-kept duplicates have drifted apart: {rule_list:?}"
            );
        }
    }
}
