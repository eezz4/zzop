//! Symbol-level dead-export detection — exported symbols that are never imported anywhere.
//! Language-neutral: only sees `zpz_core` IR types (`ImportMap`, `ReExport`, `SourceSymbolKind`); a parser
//! crate supplies each file's exports, imports, re-exports, dynamic imports, and used-identifier names.
//!
//! ## What counts as a "use"
//! An export `file#name` is alive when a named import binds it directly; a namespace import or dynamic
//! `import("./x")` targets its file at all (the whole file is wildcarded — every export alive); a
//! re-export chain carries a live root down through barrels; or the re-export originates from an
//! **entry file** (`is_entry_file`) — an `index.ts` re-exporting `impl` seeds it as live with zero
//! in-repo importers, since the entry exposes it as public API. `default` exports match the synthetic
//! `file#default` key. Separately, `reason` is same-file-only: `InFileOnly` when the name appears in the
//! file's own `used_names`, `Unused` otherwise — `used_names` comes from the parser's
//! `parse_local_identifier_refs` alone; import/re-export data drives liveness only.
//!
//! ## Exemptions
//! Entry/index/framework-convention files, test/story/ambient-declaration files, and tool-entry files
//! (config loaded by its own tool, not imported, e.g. `vite.config.ts` — see
//! `unreachable::is_tool_entry_file`) never contribute a dead candidate; see `is_entry_or_test` for the
//! full pattern list.
//!
//! ## Engine wiring
//! `dead_export_findings` shapes `find_dead_exports`'s results into `Finding`s for the `"dead-exports"`
//! native analysis; the engine layer owns the disk re-read/re-parse step this crate stays free of.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use zpz_core::{Finding, ImportMap, ReExport, Severity, SourceSymbolKind};

use crate::unreachable::is_tool_entry_file;

/// One exported symbol a file offers as a dead-export candidate.
#[derive(Debug, Clone)]
pub struct DeadExportCandidate {
    pub name: String,
    pub kind: SourceSymbolKind,
    /// `export default function Foo() {}` — also matchable via the file's `#default` import key.
    pub is_default: bool,
}

/// One file's contribution to `find_dead_exports`.
#[derive(Debug, Clone, Default)]
pub struct DeadExportInputFile {
    pub file: String,
    pub exports: Vec<DeadExportCandidate>,
    pub imports: ImportMap,
    /// `export { X } from "./a"` / `export * from "./a"`.
    pub re_exports: Vec<ReExport>,
    /// `import("./a")` dynamic-import specifiers.
    pub dynamic_imports: Vec<String>,
    /// Identifier names referenced anywhere in the file (see module doc's `used_names` paragraph).
    pub used_names: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeadExportReason {
    /// Never referenced anywhere — a deletion candidate.
    Unused,
    /// Referenced only within its own file — an un-export candidate.
    InFileOnly,
}

/// One dead-export finding, with no line number attached — a caller looks one up by `(file, name)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadExport {
    pub file: String,
    pub name: String,
    pub kind: SourceSymbolKind,
    pub reason: DeadExportReason,
}

/// Detects exported symbols that are never imported anywhere. `resolve_file` resolves a specifier to its
/// canonical file path, or `None` for an external module — see the module doc for what counts as a use.
pub fn find_dead_exports<F>(files: &[DeadExportInputFile], resolve_file: F) -> Vec<DeadExport>
where
    F: Fn(&str, &str) -> Option<String>,
{
    let mut wildcard_files: HashSet<String> = HashSet::new();
    let mut imported_keys: HashSet<String> = HashSet::new();
    // re_export_chain[barrel_file] = [(local_alias, target_file, original_name)] — for chain resolution.
    let mut re_export_chain: HashMap<String, Vec<(String, String, String)>> = HashMap::new();

    for f in files {
        for binding in f.imports.values() {
            let Some(target) = resolve_file(&binding.specifier, &f.file) else {
                continue;
            };
            if binding.original == "*" {
                wildcard_files.insert(target);
            } else {
                imported_keys.insert(format!("{target}#{}", binding.original));
            }
        }
        for spec in &f.dynamic_imports {
            if let Some(target) = resolve_file(spec, &f.file) {
                wildcard_files.insert(target);
            }
        }
        for r in &f.re_exports {
            let Some(target) = resolve_file(&r.specifier, &f.file) else {
                continue;
            };
            if r.original == "*" {
                wildcard_files.insert(target);
                continue;
            }
            re_export_chain.entry(f.file.clone()).or_default().push((
                r.local_alias.clone(),
                target.clone(),
                r.original.clone(),
            ));
            // An entry-file re-export is a live root without an in-repo importer — see module doc.
            if is_entry_file(&f.file) && !is_excluded_file(&f.file) {
                imported_keys.insert(format!("{target}#{}", r.original));
            }
        }
    }

    propagate_re_exports(&mut imported_keys, &mut wildcard_files, &re_export_chain);

    let mut dead: Vec<DeadExport> = Vec::new();
    for f in files {
        if is_entry_or_test(&f.file) {
            continue;
        }
        if wildcard_files.contains(&f.file) {
            continue;
        }
        for exp in &f.exports {
            if imported_keys.contains(&format!("{}#{}", f.file, exp.name)) {
                continue;
            }
            // `export default function Foo()` is also importable as `import Foo from "..."` — match the key.
            if exp.is_default && imported_keys.contains(&format!("{}#default", f.file)) {
                continue;
            }
            let reason = if f.used_names.contains(&exp.name) {
                DeadExportReason::InFileOnly
            } else {
                DeadExportReason::Unused
            };
            dead.push(DeadExport {
                file: f.file.clone(),
                name: exp.name.clone(),
                kind: exp.kind,
                reason,
            });
        }
    }
    dead.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.name.cmp(&b.name)));
    dead
}

/// When `barrel#X` is imported, the source it re-exports is alive too — a fixpoint loop resolves multi-hop chains.
fn propagate_re_exports(
    imported_keys: &mut HashSet<String>,
    wildcard_files: &mut HashSet<String>,
    chain: &HashMap<String, Vec<(String, String, String)>>,
) {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = imported_keys.iter().cloned().collect();
    while let Some(key) = queue.pop() {
        if !visited.insert(key.clone()) {
            continue;
        }
        let Some(hash_idx) = key.rfind('#') else {
            continue;
        };
        let file = &key[..hash_idx];
        let name = &key[hash_idx + 1..];
        let Some(edges) = chain.get(file) else {
            continue;
        };
        for (local_alias, target_file, original_name) in edges {
            if local_alias != name {
                continue;
            }
            let next_key = format!("{target_file}#{original_name}");
            if imported_keys.contains(&next_key) {
                continue;
            }
            imported_keys.insert(next_key.clone());
            queue.push(next_key);
        }
    }
    // wildcard_files propagate through the chain too, via the same fixpoint, to reach further hops.
    let mut changed = true;
    while changed {
        changed = false;
        let current: Vec<String> = wildcard_files.iter().cloned().collect();
        for file in current {
            let Some(edges) = chain.get(&file) else {
                continue;
            };
            for (_, target_file, _) in edges {
                if wildcard_files.insert(target_file.clone()) {
                    changed = true;
                }
            }
        }
    }
}

fn is_entry_file(path: &str) -> bool {
    entry_patterns().iter().any(|re| re.is_match(path))
}

fn is_excluded_file(path: &str) -> bool {
    exclude_patterns().iter().any(|re| re.is_match(path))
}

fn is_entry_or_test(path: &str) -> bool {
    // `is_tool_entry_file` covers tool-config files loaded by their own tool rather than imported.
    is_entry_file(path) || is_excluded_file(path) || is_tool_entry_file(path)
}

fn entry_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"(^|/)index\.(ts|tsx|js|jsx)$",
            r"(^|/)main\.(ts|tsx)$",
            r"(^|/)App\.(ts|tsx)$",
            r"Page\.(ts|tsx)$",
            r"(^|/)apiRoutes\.(ts|tsx)$",
            // Next.js App Router convention files — called by the framework rather than imported.
            r"(^|/)(page|layout|loading|error|global-error|not-found|template|default|route)\.(ts|tsx)$",
            r"(^|/)(sitemap|robots|manifest|opengraph-image|twitter-image|icon|apple-icon)\.(ts|tsx)$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

fn exclude_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"\.(test|spec)\.(ts|tsx|js|jsx)$",
            r"\.stories\.(ts|tsx|js|jsx)$",
            r"/__test__/",
            r"/__mocks__/",
            r"\.d\.ts$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

/// Converts every `find_dead_exports` result into a `Finding` at its symbol's declaration line.
pub fn dead_export_findings(
    dead: Vec<DeadExport>,
    symbol_lines: &HashMap<(&str, &str), u32>,
) -> Vec<Finding> {
    dead.into_iter()
        .map(|d| dead_export_to_finding(symbol_lines, d))
        .collect()
}

fn dead_export_to_finding(symbol_lines: &HashMap<(&str, &str), u32>, d: DeadExport) -> Finding {
    let line = symbol_lines
        .get(&(d.file.as_str(), d.name.as_str()))
        .copied()
        .unwrap_or(1);
    let message = format!(
        "exported {} '{}' is {} ({}). {} Disable via rule config `disabled_rules: [\"dead-exports\"]` \
         if this is public API consumed outside this repo (e.g. published to npm) — such consumers are \
         invisible to this in-repo import graph.",
        kind_label(d.kind),
        d.name,
        match d.reason {
            DeadExportReason::Unused => "never imported anywhere",
            DeadExportReason::InFileOnly => "only referenced within its own file",
        },
        reason_label(d.reason),
        match d.reason {
            DeadExportReason::Unused => "Delete it, or export it from somewhere it's actually consumed.",
            DeadExportReason::InFileOnly => "Drop the `export` keyword to make the un-used-elsewhere status explicit.",
        },
    );
    Finding {
        rule_id: "dead-exports".to_string(),
        severity: Severity::Info,
        file: d.file.clone(),
        line,
        message,
        data: serde_json::to_value(&d).ok(),
    }
}

fn kind_label(kind: SourceSymbolKind) -> &'static str {
    match kind {
        SourceSymbolKind::Function => "function",
        SourceSymbolKind::Class => "class",
        SourceSymbolKind::Const => "const",
        SourceSymbolKind::Type => "type",
        SourceSymbolKind::Interface => "interface",
    }
}

fn reason_label(reason: DeadExportReason) -> &'static str {
    match reason {
        DeadExportReason::Unused => "deletion candidate",
        DeadExportReason::InFileOnly => "un-export candidate",
    }
}

#[cfg(test)]
mod tests {
    //! Exercises `find_dead_exports` against hand-built fixtures — imports, barrel/aliased re-export
    //! chains, entry-file live roots, default-export matching, and the `Unused` vs `InFileOnly` split.
    use super::*;
    use zpz_core::ImportBinding;

    fn resolve(spec: &str, _from: &str) -> Option<String> {
        Some(spec.strip_prefix("./").unwrap_or(spec).to_string())
    }

    fn resolve_relative_only(spec: &str, _from: &str) -> Option<String> {
        if spec.starts_with('.') {
            Some(spec.strip_prefix("./").unwrap_or(spec).to_string())
        } else {
            None
        }
    }

    fn export(name: &str, kind: SourceSymbolKind) -> DeadExportCandidate {
        DeadExportCandidate {
            name: name.to_string(),
            kind,
            is_default: false,
        }
    }

    fn default_export(name: &str, kind: SourceSymbolKind) -> DeadExportCandidate {
        DeadExportCandidate {
            name: name.to_string(),
            kind,
            is_default: true,
        }
    }

    fn file(name: &str, exports: Vec<DeadExportCandidate>) -> DeadExportInputFile {
        DeadExportInputFile {
            file: name.to_string(),
            exports,
            imports: ImportMap::new(),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            used_names: HashSet::new(),
        }
    }

    fn import_of(specifier: &str, original: &str) -> ImportMap {
        let mut m = ImportMap::new();
        m.insert(
            "local".to_string(),
            ImportBinding {
                specifier: specifier.to_string(),
                original: original.to_string(),
                deferred: false,
                type_only: false,
            },
        );
        m
    }

    fn reexport(specifier: &str, original: &str, local_alias: &str) -> ReExport {
        ReExport {
            specifier: specifier.to_string(),
            original: original.to_string(),
            local_alias: local_alias.to_string(),
        }
    }

    #[test]
    fn exported_symbol_that_is_imported_is_not_dead() {
        let files = vec![
            file("a.ts", vec![export("foo", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                imports: import_of("./a.ts", "foo"),
                ..file("b.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn function_const_not_imported_anywhere_is_dead() {
        let files = vec![
            file(
                "a.ts",
                vec![
                    export("used", SourceSymbolKind::Function),
                    export("unused", SourceSymbolKind::Function),
                ],
            ),
            DeadExportInputFile {
                imports: import_of("./a.ts", "used"),
                ..file("b.ts", vec![])
            },
        ];
        let dead = find_dead_exports(&files, resolve);
        assert_eq!(
            dead,
            vec![DeadExport {
                file: "a.ts".to_string(),
                name: "unused".to_string(),
                kind: SourceSymbolKind::Function,
                reason: DeadExportReason::Unused,
            }]
        );
    }

    #[test]
    fn type_interface_are_also_dead_candidates() {
        let files = vec![file(
            "a.ts",
            vec![
                export("MyType", SourceSymbolKind::Type),
                export("MyShape", SourceSymbolKind::Interface),
                export("myFn", SourceSymbolKind::Function),
            ],
        )];
        let mut names: Vec<String> = find_dead_exports(&files, resolve)
            .into_iter()
            .map(|d| d.name)
            .collect();
        names.sort();
        assert_eq!(names, vec!["MyShape", "MyType", "myFn"]);
    }

    #[test]
    fn type_export_is_alive_when_imported_at_least_once() {
        let files = vec![
            file("a.ts", vec![export("MyType", SourceSymbolKind::Type)]),
            DeadExportInputFile {
                imports: import_of("./a.ts", "MyType"),
                ..file("b.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn ambient_declaration_is_excluded_from_dead_candidates() {
        let files = vec![file(
            "globals.d.ts",
            vec![export("MyAmbient", SourceSymbolKind::Type)],
        )];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn tool_config_files_default_export_is_excluded_from_dead_candidates() {
        // Loaded directly by its own tool, never imported — the default export must not read as dead.
        let files = vec![file(
            "vite.config.ts",
            vec![default_export("config", SourceSymbolKind::Const)],
        )];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn namespace_import_treats_all_exports_of_that_file_as_alive() {
        let files = vec![
            file(
                "a.ts",
                vec![
                    export("x", SourceSymbolKind::Function),
                    export("y", SourceSymbolKind::Function),
                ],
            ),
            DeadExportInputFile {
                imports: import_of("./a.ts", "*"),
                ..file("b.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn entry_files_are_not_dead_candidates() {
        let files = vec![
            file(
                "src/index.ts",
                vec![export("x", SourceSymbolKind::Function)],
            ),
            file(
                "pages/HomePage.tsx",
                vec![export("HomePage", SourceSymbolKind::Function)],
            ),
            file("App.tsx", vec![export("App", SourceSymbolKind::Function)]),
            file(
                "api/apiRoutes.ts",
                vec![export("routes", SourceSymbolKind::Const)],
            ),
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn nextjs_app_router_convention_files_are_framework_entries() {
        let files = vec![
            file(
                "app/(lang)/[lang]/about/page.tsx",
                vec![
                    default_export("AboutPage", SourceSymbolKind::Function),
                    export("generateMetadata", SourceSymbolKind::Function),
                    export("generateStaticParams", SourceSymbolKind::Function),
                    export("dynamicParams", SourceSymbolKind::Const),
                ],
            ),
            file(
                "app/(lang)/[lang]/error.tsx",
                vec![default_export("ErrorPage", SourceSymbolKind::Function)],
            ),
            file(
                "app/api/x/route.ts",
                vec![export("GET", SourceSymbolKind::Function)],
            ),
            file(
                "app/sitemap.ts",
                vec![default_export("sitemap", SourceSymbolKind::Function)],
            ),
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn test_and_mock_dirs_are_excluded_at_source_stage() {
        let files = vec![file(
            "src/__test__/x.test.ts",
            vec![export("fixture", SourceSymbolKind::Function)],
        )];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn default_export_is_tracked() {
        let files = vec![
            file("a.ts", vec![export("default", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                imports: import_of("./a.ts", "default"),
                ..file("b.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn external_module_import_is_ignored() {
        let files = vec![
            file("a.ts", vec![export("foo", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                imports: import_of("react", "foo"),
                ..file("b.ts", vec![])
            },
        ];
        let dead = find_dead_exports(&files, resolve_relative_only);
        assert_eq!(
            dead,
            vec![DeadExport {
                file: "a.ts".to_string(),
                name: "foo".to_string(),
                kind: SourceSymbolKind::Function,
                reason: DeadExportReason::Unused,
            }]
        );
    }

    #[test]
    fn barrel_re_export_chain_resolves_source_as_alive() {
        let files = vec![
            file("a.ts", vec![export("Foo", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                re_exports: vec![reexport("./a.ts", "Foo", "Foo")],
                ..file("barrel/index.ts", vec![])
            },
            DeadExportInputFile {
                imports: import_of("./barrel/index.ts", "Foo"),
                ..file("consumer.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn aliased_re_export_consumer_imports_alias_source_is_alive() {
        let files = vec![
            file("a.ts", vec![export("Orig", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                re_exports: vec![reexport("./a.ts", "Orig", "Alias")],
                ..file("barrel/index.ts", vec![])
            },
            DeadExportInputFile {
                imports: import_of("./barrel/index.ts", "Alias"),
                ..file("consumer.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn star_re_export_wildcards_the_whole_source_file() {
        let files = vec![
            file(
                "a.ts",
                vec![
                    export("x", SourceSymbolKind::Function),
                    export("y", SourceSymbolKind::Const),
                ],
            ),
            DeadExportInputFile {
                re_exports: vec![reexport("./a.ts", "*", "*")],
                ..file("barrel/index.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn dynamic_import_wildcards_the_whole_target_file() {
        let files = vec![
            file(
                "a.ts",
                vec![
                    export("x", SourceSymbolKind::Function),
                    export("y", SourceSymbolKind::Const),
                ],
            ),
            DeadExportInputFile {
                dynamic_imports: vec!["./a.ts".to_string()],
                ..file("consumer.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn named_default_export_is_alive_via_default_import() {
        let files = vec![
            file(
                "a.ts",
                vec![default_export("Foo", SourceSymbolKind::Function)],
            ),
            DeadExportInputFile {
                imports: import_of("./a.ts", "default"),
                ..file("b.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn reason_in_file_only_when_referenced_only_within_the_file() {
        let files = vec![DeadExportInputFile {
            used_names: HashSet::from(["HELPER".to_string()]),
            ..file("a.ts", vec![export("HELPER", SourceSymbolKind::Const)])
        }];
        assert_eq!(
            find_dead_exports(&files, resolve),
            vec![DeadExport {
                file: "a.ts".to_string(),
                name: "HELPER".to_string(),
                kind: SourceSymbolKind::Const,
                reason: DeadExportReason::InFileOnly,
            }]
        );
    }

    #[test]
    fn reason_unused_when_referenced_nowhere() {
        let files = vec![file(
            "a.ts",
            vec![export("HELPER", SourceSymbolKind::Const)],
        )];
        let dead = find_dead_exports(&files, resolve);
        assert_eq!(dead[0].reason, DeadExportReason::Unused);
    }

    #[test]
    fn named_default_export_without_any_default_import_is_dead() {
        let files = vec![file(
            "a.ts",
            vec![default_export("Foo", SourceSymbolKind::Function)],
        )];
        assert_eq!(
            find_dead_exports(&files, resolve),
            vec![DeadExport {
                file: "a.ts".to_string(),
                name: "Foo".to_string(),
                kind: SourceSymbolKind::Function,
                reason: DeadExportReason::Unused,
            }]
        );
    }

    #[test]
    fn entry_re_export_is_a_live_root_even_with_no_consumer() {
        // An entry file re-exporting `impl` with no in-repo importer is still public API, not dead.
        let files = vec![
            file("impl.ts", vec![export("impl", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                re_exports: vec![reexport("./impl.ts", "impl", "impl")],
                ..file("index.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn entry_re_export_root_propagates_across_a_deeper_barrel_hop() {
        let files = vec![
            file("impl.ts", vec![export("impl", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                re_exports: vec![reexport("./impl.ts", "impl", "impl")],
                ..file("mid.ts", vec![])
            },
            DeadExportInputFile {
                re_exports: vec![reexport("./mid.ts", "impl", "impl")],
                ..file("index.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn re_export_from_a_non_entry_file_is_not_a_live_root_by_itself() {
        // A non-entry re-exporter alone isn't a live root; a real import must exist somewhere in the chain.
        let files = vec![
            file("impl.ts", vec![export("impl", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                re_exports: vec![reexport("./impl.ts", "impl", "impl")],
                ..file("reexporter.ts", vec![])
            },
        ];
        assert_eq!(
            find_dead_exports(&files, resolve),
            vec![DeadExport {
                file: "impl.ts".to_string(),
                name: "impl".to_string(),
                kind: SourceSymbolKind::Function,
                reason: DeadExportReason::Unused,
            }]
        );
    }

    #[test]
    fn re_export_chain_propagates_across_2_hops() {
        let files = vec![
            file("a.ts", vec![export("Foo", SourceSymbolKind::Function)]),
            DeadExportInputFile {
                re_exports: vec![reexport("./a.ts", "Foo", "Foo")],
                ..file("mid.ts", vec![])
            },
            DeadExportInputFile {
                re_exports: vec![reexport("./mid.ts", "Foo", "Foo")],
                ..file("barrel/index.ts", vec![])
            },
            DeadExportInputFile {
                imports: import_of("./barrel/index.ts", "Foo"),
                ..file("consumer.ts", vec![])
            },
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }
}
