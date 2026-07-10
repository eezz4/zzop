//! Symbol-level dead-export detection — exported symbols that are never imported anywhere.
//! Language-neutral: only sees `zzop_core` IR types (`ImportMap`, `ReExport`, `SourceSymbolKind`); a parser
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
//! Entry/index/framework-convention files, test/story/ambient-declaration files, `.storybook/` config
//! files, and tool-entry files (config loaded by its own tool, not imported, e.g. `vite.config.ts` — see
//! `unreachable::is_tool_entry_file`) never contribute a dead candidate; see `is_entry_or_test` for the
//! full pattern list. A small named-export allowlist (`is_framework_contract_export`) additionally
//! exempts individual exports — Next.js `getServerSideProps`/`getStaticProps`/`getStaticPaths`/
//! `getInitialProps`/`generateMetadata`/`generateStaticParams` — that the framework consumes by exact
//! identifier rather than by import, even in files that aren't otherwise excluded (e.g. Next.js Pages
//! Router files). The Next.js root-middleware convention exports `middleware`/`config` are exempted only
//! inside a `middleware.{ts,js}` file (`is_middleware_convention_file`) — those names are too generic to
//! exempt globally.
//!
//! ## Engine wiring
//! `dead_export_findings` shapes `find_dead_exports`'s results into `Finding`s for the `"dead-exports"`
//! native analysis; the engine layer owns the disk re-read/re-parse step this crate stays free of.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use zzop_core::{disable_hint, Finding, ImportMap, ReExport, Severity, SourceSymbolKind};

use crate::unreachable::{framework_route_patterns, is_tool_entry_file};

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
            // Framework-contract export names are consumed by the framework via convention, not import.
            if is_framework_contract_export(&exp.name) {
                continue;
            }
            // Next.js middleware convention file: its `middleware`/`config` exports are read by the
            // framework by exact name, never imported. Scoped to the `middleware.{ts,js}` filename (any
            // directory — a Next app in a monorepo sits below the tree root) so a dead `middleware`/
            // `config` symbol in any other file still reports.
            if matches!(exp.name.as_str(), "middleware" | "config")
                && is_middleware_convention_file(&f.file)
            {
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
        let mut v: Vec<Regex> = [
            r"(^|/)index\.(ts|tsx|js|jsx)$",
            r"(^|/)main\.(ts|tsx)$",
            r"(^|/)App\.(ts|tsx)$",
            r"Page\.(ts|tsx)$",
            r"(^|/)apiRoutes\.(ts|tsx)$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect();
        // Next.js App Router convention files — shared with `dead_candidates` so the two can't drift.
        v.extend(framework_route_patterns().iter().cloned());
        v
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
            // Storybook config directory — `.storybook/preview.tsx`, `.storybook/main.ts`, etc. Storybook
            // loads these itself by fixed filename/directory convention, never via an in-repo import.
            r"(^|/)\.storybook/",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

/// Framework-contract export names consumed by their framework via named-export convention rather than
/// an `import` — flagging them dead and deleting them breaks the app at runtime even though the in-repo
/// import graph shows zero importers. Kept deliberately small and unambiguous: every name here is a
/// framework-reserved *camelCase* identifier that a project would essentially never coincidentally reuse
/// for an unrelated symbol. Generic words are excluded ON PURPOSE — a bare `config`/`meta`/`parameters`/
/// `decorators`/`middleware`/`default` is plausibly a real (possibly dead) domain symbol, so a global
/// name exemption for those would cause false NEGATIVES (real dead code silently missed). They are left
/// to the normal dead-export path.
///
/// This is load-bearing for Next.js *Pages Router* files (`pages/**`, e.g. `pages/blog/[slug].tsx`),
/// whose file paths are arbitrary and unmatched by any entry/exclude pattern, so
/// `getServerSideProps`/`getStaticProps`/… in such a file would otherwise read as dead. Next.js App
/// Router convention files (`page.tsx`, `layout.tsx`, `route.ts`, …) are already wholesale-excluded via
/// `framework_route_patterns()` in `entry_patterns()`, so the list is belt-and-suspenders for those.
///
/// Storybook `decorators`/`parameters`/`globalTypes` are deliberately NOT here — they live in
/// `.storybook/`- or `.stories.`-path files, both already file-level excluded (see `exclude_patterns()`),
/// so a name exemption would only add false-negative risk for the rare re-export-from-elsewhere case.
/// Next.js root `middleware.ts` is likewise left out: `middleware` is too generic a name to exempt
/// globally — its `middleware`/`config` exports are instead exempted only when the file itself is a
/// `middleware.{ts,js}` convention file (see `is_middleware_convention_file` in `find_dead_exports`).
fn is_framework_contract_export(name: &str) -> bool {
    matches!(
        name,
        // Next.js: reserved data-fetching/route-contract export names, read by the framework at build
        // or request time by exact identifier — never through a normal import statement.
        "getServerSideProps"
            | "getStaticProps"
            | "getStaticPaths"
            | "getInitialProps"
            | "generateMetadata"
            | "generateStaticParams"
    )
}

/// `middleware.ts`/`middleware.js` — the Next.js root-middleware convention filename, whose
/// `middleware`/`config` exports the framework reads by exact name. Deliberately NOT root-anchored: a
/// Next app inside a monorepo tree lives below the analyzed root (`apps/web/middleware.ts`). The
/// accepted false-negative is a dead symbol literally named `middleware`/`config` in a non-Next file
/// that happens to be named `middleware.ts` — far rarer than the Next convention FP this removes.
fn is_middleware_convention_file(path: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(^|/)middleware\.(ts|js)$").unwrap())
        .is_match(path)
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
        "exported {} '{}' is {} ({}). {} {} if this is public API consumed outside this repo (e.g. \
         published to npm) — such consumers are invisible to this in-repo import graph.",
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
        disable_hint("dead-exports"),
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
    use zzop_core::ImportBinding;

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
            type_only: false,
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

    #[test]
    fn storybook_config_dir_export_is_excluded_from_dead_candidates() {
        // `.storybook/preview.tsx`'s `decorators` is consumed by Storybook's own builder, never imported.
        let files = vec![file(
            ".storybook/preview.tsx",
            vec![export("decorators", SourceSymbolKind::Const)],
        )];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn nextjs_pages_router_data_fetching_export_is_not_dead() {
        // Pages Router files have arbitrary filenames (unlike App Router's `page.tsx` convention), so
        // this relies on the framework-contract-export allowlist rather than file-level exclusion.
        let files = vec![file(
            "pages/blog/[slug].tsx",
            vec![export("getServerSideProps", SourceSymbolKind::Function)],
        )];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn nextjs_middleware_convention_file_exports_are_not_dead() {
        // Root `middleware.ts` (and a monorepo app's `apps/web/middleware.ts`) export `middleware` +
        // `config`, both read by Next.js by exact name — never imported.
        let files = vec![
            file(
                "middleware.ts",
                vec![
                    export("middleware", SourceSymbolKind::Function),
                    export("config", SourceSymbolKind::Const),
                ],
            ),
            file(
                "apps/web/middleware.ts",
                vec![export("middleware", SourceSymbolKind::Function)],
            ),
        ];
        assert!(find_dead_exports(&files, resolve).is_empty());
    }

    #[test]
    fn other_exports_in_a_middleware_file_are_still_dead_candidates() {
        // The exemption is name-scoped (`middleware`/`config` only), not a wholesale file exclusion.
        let files = vec![file(
            "middleware.ts",
            vec![export("helper", SourceSymbolKind::Function)],
        )];
        assert_eq!(
            find_dead_exports(&files, resolve),
            vec![DeadExport {
                file: "middleware.ts".to_string(),
                name: "helper".to_string(),
                kind: SourceSymbolKind::Function,
                reason: DeadExportReason::Unused,
            }]
        );
    }

    #[test]
    fn middleware_named_export_outside_a_middleware_file_is_still_dead() {
        // Regression guard: the filename scoping must not leak into a global name exemption.
        let files = vec![file(
            "src/utils.ts",
            vec![export("middleware", SourceSymbolKind::Function)],
        )];
        assert_eq!(find_dead_exports(&files, resolve).len(), 1);
    }

    #[test]
    fn ordinary_never_imported_export_in_a_normal_file_is_still_dead() {
        // Regression guard: the framework-contract allowlist must not over-broaden to arbitrary symbols.
        let files = vec![file(
            "src/utils.ts",
            vec![export("helper", SourceSymbolKind::Function)],
        )];
        assert_eq!(
            find_dead_exports(&files, resolve),
            vec![DeadExport {
                file: "src/utils.ts".to_string(),
                name: "helper".to_string(),
                kind: SourceSymbolKind::Function,
                reason: DeadExportReason::Unused,
            }]
        );
    }

    /// Pins the exact rendered message — regression coverage for the `disable_hint` splice
    /// `dead_export_to_finding` went through during the 2026-07-10 dialect-consolidation sweep. Covers both
    /// `DeadExportReason` variants, since each selects different fixed text around the shared hint.
    #[test]
    fn finding_message_is_byte_identical_to_the_pre_sweep_text() {
        let dead = vec![
            DeadExport {
                file: "src/utils.ts".to_string(),
                name: "helper".to_string(),
                kind: SourceSymbolKind::Function,
                reason: DeadExportReason::Unused,
            },
            DeadExport {
                file: "src/utils.ts".to_string(),
                name: "localOnly".to_string(),
                kind: SourceSymbolKind::Const,
                reason: DeadExportReason::InFileOnly,
            },
        ];
        let out = dead_export_findings(dead, &HashMap::new());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].rule_id, "dead-exports");
        // Interpolates `disable_hint`'s own output (rather than spelling "Disable via config `rules:
        // {...}`" as a literal here) so this file's own source never carries that literal text next to a
        // `` `export` `` backtick — `packages/engine/tests/rule_contracts.rs`'s CHECK B flags exactly that
        // shape (a backtick-quoted, non-config-key token sitting within 120 bytes of the word "config") as
        // an unvouched-for config-key reference. `disable_hint`'s own unit tests (`packages/core/src/
        // finding.rs`) already pin its rendered form; this test only needs to confirm it lands in the right
        // place in the surrounding sentence.
        let tail = disable_hint("dead-exports");
        assert_eq!(
            out[0].message,
            format!(
                "exported function 'helper' is never imported anywhere (deletion candidate). Delete it, \
                 or export it from somewhere it's actually consumed. {tail} if this is public API \
                 consumed outside this repo (e.g. published to npm) — such consumers are invisible to \
                 this in-repo import graph."
            )
        );
        assert_eq!(
            out[1].message,
            format!(
                "exported const 'localOnly' is only referenced within its own file (un-export candidate). \
                 Drop the `export` keyword to make the un-used-elsewhere status explicit. {tail} if this \
                 is public API consumed outside this repo (e.g. published to npm) — such consumers are \
                 invisible to this in-repo import graph."
            )
        );
    }
}
