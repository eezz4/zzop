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
//! full pattern list. A file the engine marks `is_generated` (an author-declared `@generated` /
//! "auto-generated" banner in its head, detected by the engine's `has_generated_banner`) is likewise skipped whole:
//! its exports are regenerated, never hand-edited, so an "un-export the unused" finding there is
//! non-actionable noise. The flag rides in from the engine because the rule crate stays free of file
//! text; a generated file's *imports* still count (they keep other files' exports alive).
//! A small named-export allowlist (`is_framework_contract_export`) additionally
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

mod findings;
mod patterns;
#[cfg(test)]
mod tests;

pub use findings::dead_export_findings;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use zzop_core::{ImportMap, ReExport, SourceSymbolKind};

use patterns::{
    is_entry_file, is_entry_or_test, is_excluded_file, is_framework_contract_export,
    is_middleware_convention_file,
};

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
    /// The engine detected an author-declared `@generated`/"DO NOT EDIT" banner in this file's head.
    /// When set, the file's exports are skipped whole (its imports still count) — see module doc.
    pub is_generated: bool,
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
        // Author-declared generated file: its exports are regenerated, never hand-edited, so an
        // un-export finding is non-actionable. Skipped in this dead-check loop only — the first loop
        // above already consumed its imports/re-exports, keeping whatever it uses alive.
        if f.is_generated {
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
