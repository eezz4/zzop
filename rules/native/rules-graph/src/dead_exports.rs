//! Symbol-level export-import reconciliation — exported symbols that no other file imports.
//!
//! The analysis id is `unimported-export`, not `dead-exports` (its former spelling, kept only as this
//! module's Rust path): half the output is `DeadExportReason::InFileOnly`, which reports a symbol that IS
//! referenced — in its own file — and advises dropping the `export` keyword rather than deleting the
//! symbol. "Dead" was a claim that one of the two arms contradicts; "unimported" is what both arms
//! actually measure. The old id is recorded in `VERSIONING.md`.
//!
//! Language-neutral: only sees `zzop_core` IR types (`ImportMap`, `ReExport`, `SourceSymbolKind`); a parser
//! crate supplies each file's exports, imports, re-exports, dynamic imports, and used-identifier names.
//!
//! ## What counts as a "use"
//! An export `file#name` is alive when a named import binds it directly; a namespace import or dynamic
//! `import("./x")` targets its file at all (the whole file is wildcarded — every export alive); a
//! re-export chain carries a live root down through barrels; or the re-export originates from an
//! **entry file** (`is_entry_file`) — an `index.ts` re-exporting `impl` seeds it as live with zero
//! in-repo importers, since the entry exposes it as public API. `default` exports match the synthetic
//! `file#default` key, and a LOCAL RENAME (`export { X as Y }`, no from-clause — see `export_aliases`)
//! matches the `file#Y` key its importers actually write. Separately, `reason` is same-file-only: `InFileOnly` when the name appears in the
//! file's own `used_names`, `Unused` otherwise — `used_names` comes from the parser's
//! `parse_local_identifier_refs` alone; import/re-export data drives liveness only.
//!
//! ## Local renames (`export { X as Y }`)
//! An export candidate is named by its DECLARATION, but a from-less `export { X as Y }` publishes it
//! under `Y` — so every importer writes `{file}#Y` while the candidate is `X`, and the two never met.
//! `export_aliases` carries that mapping. It is an extra KEY to look up, not an exemption: a renamed
//! export that nobody imports still reports, so this cannot resurrect a genuinely dead export.
//! The sibling shapes need no such mapping — `export { X as Y } from "./z"` is a re-export (its
//! `local_alias`/`original` pair already rides the chain), `export * as ns from "./z"` wildcards the
//! whole target file, and `import { A as B } from "./a"; export { B }` republishes someone else's
//! declaration, so this file offers no candidate for it at all. Language coverage: TypeScript only —
//! `unimported-export` itself only ever runs on TypeScript-dispatched files, so no other language is
//! affected either way.
//!
//! ## Public-signature exemption (type exports)
//! A `Type`/`Interface` export whose name appears in `exported_signature_names` — the parser's set of
//! names occurring in the PUBLIC SIGNATURE (parameter / return / member type-annotation positions) of
//! some exported declaration in the same file — is not reported at all. The measured shape, by far the
//! largest single noise source this rule had: `export interface XState {…}` immediately followed by
//! `export function useX(): XState`. `XState` has no in-repo importer, so the rule called it
//! `in-file-only` and advised un-exporting it — but it is part of `useX`'s public API, and
//! un-exporting it degrades every consumer that needs to name the returned type.
//!
//! **What this evidence is, precisely.** `used_names` alone CANNOT express this exemption: it is a
//! flat, position-blind set, so a type in an exported return type and a type used only as an internal
//! `useState<T>` generic look identical to it. `exported_signature_names` is a separate, position-aware
//! fact produced at parse time (`zzop_parser_typescript::parse_exported_signature_names`) precisely
//! because that distinction cannot be recovered downstream. This rule does not know WHERE a name
//! appeared — it knows only that the parser classified it as signature-reachable, and trusts that
//! classification.
//!
//! **Deliberately still reported** (the parser collects only exported declarations, and within them
//! only type-annotation positions, never a body): a type used solely inside a function body, including
//! as a generic argument in a hook with no annotated return type; and a type that only annotates an
//! UNEXPORTED declaration's field. Both are genuinely private and remain un-export candidates.
//! Language coverage: TypeScript only. Every other parser leaves the set empty, which yields no
//! exemptions — identical to the pre-existing behavior, never an error.
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
//! `dead_export_findings` shapes `find_dead_exports`'s results into `Finding`s for the `"unimported-export"`
//! native analysis; the engine layer owns the disk re-read/re-parse step this crate stays free of.

mod findings;
mod patterns;
mod propagate;
#[cfg(test)]
mod tests;

pub use findings::dead_export_findings;

use propagate::propagate_re_exports;

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
    /// Names appearing in the PUBLIC SIGNATURE of some exported declaration in this file — the
    /// position-aware companion `used_names` cannot be. Drives the public-signature exemption (see
    /// module doc). Empty for a parser that does not produce it, which simply means no exemptions.
    pub exported_signature_names: HashSet<String>,
    /// Local export RENAMES — `(local declaration name, public export name)` for every from-less
    /// `export { X as Y }` in this file (`zzop_parser_typescript::parse_dead_export_facts`). An
    /// export candidate is named by its DECLARATION (`X`), but an importer's key is the PUBLIC name
    /// (`Y`); this is the only place those two meet. NOT a liveness grant on its own — a rename
    /// nobody imports leaves the export dead, exactly as before (see `find_dead_exports`).
    /// Empty for a parser that does not produce it, which simply restores the pre-existing behavior.
    pub export_aliases: Vec<(String, String)>,
    /// The engine detected an author-declared machine-generated banner in this file's head —
    /// `@generated`, "auto-generated", "Code generated by … DO NOT EDIT.", and friends. NOT a bare
    /// "DO NOT EDIT": `crates/engine/src/generated_banner.rs` deliberately refuses that one (a
    /// hand-written "DO NOT EDIT directly — change it via the admin UI" header carries it) and pins the
    /// refusal with its own test. This doc used to spell the pair `@generated`/"DO NOT EDIT", which
    /// advertised an exemption the detector does not grant.
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
            // A local rename (`export { X as Y }`) publishes this declaration under `Y`, so every
            // importer's key is `{file}#Y` and never `{file}#X`. Deliberately still an IMPORT check,
            // not a blanket exemption: an export renamed but imported by nobody stays dead.
            if f.export_aliases.iter().any(|(local, public)| {
                local == &exp.name && imported_keys.contains(&format!("{}#{public}", f.file))
            }) {
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
            // Public-signature exemption: a TYPE named in an exported declaration's signature is
            // public API — un-exporting it would break any consumer that needs to name the type.
            // Scoped to type-shaped kinds: a VALUE's name reaching a type position would need
            // `typeof`, which this evidence deliberately does not model.
            if matches!(
                exp.kind,
                SourceSymbolKind::Type | SourceSymbolKind::Interface
            ) && f.exported_signature_names.contains(&exp.name)
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
