//! Project-wide (whole-corpus) ASP.NET Core HTTP route PROVIDES resolution — a superset of the per-file
//! `adapters::provides::extract_csharp_http_provides` pass resolving one fact invisible to a single file:
//! a NON-LITERAL route constant. A `[HttpGet(Routes.List)]` method path, or a `[Route(ApiRoutes.Base)]`
//! class prefix, references a C# `const string`/`static readonly string` — a compile-time constant that is
//! valid in an attribute argument but usually DECLARED IN ANOTHER FILE (a `static class Routes { ... }`).
//! The per-file pass has no corpus to resolve it against, so it DROPS the route (method path) / BLOCKS the
//! class (prefix). This pass collects every class's own constants across the whole corpus and resolves the
//! reference, exactly as `zzop_parser_java_21::project` already does for Spring's `@RequestMapping` constants.
//!
//! ## v1 scope (deliberately SIMPLER than the Java resolver — see the crate's task brief)
//! - **No `extends`-chain / base-class routing.** ASP.NET route inheritance is rare; a controller resolves
//!   its OWN class-prefix + method-path constants only, never a superclass's (Java's CE-split has no C# analog).
//! - **No `+`-concatenation.** Only a SIMPLE constant reference resolving to a single string literal is
//!   handled; a concatenated/computed `const` value stays unresolved (its route dropped and counted).
//! - **Reference forms.** A qualified `ClassName.CONST` resolves by `(ClassName, CONST)`; a bare `CONST`
//!   resolves globally by name, with the SAME ambiguity rule Java uses — a name declared in 2+ classes is
//!   ambiguous and skipped (`resolve::build_bare_index`).
//!
//! ## Partial classes (a C#-specific merge the Java resolver has no analog for)
//! C#'s `partial class` legitimately splits ONE class across multiple files — a controller commonly carries
//! its `[ApiController]`/`[Route]` on one half and its route methods on another. So two same-name rows are NOT
//! automatically ambiguous the way Java (which has no partial classes) treats them: when EVERY row sharing a
//! simple name is `partial`, they are MERGED into one `ClassRow` (union constants first-wins, concat methods,
//! `is_controller = any`, first non-empty `prefix` wins; halves processed in deterministic file order). Only
//! when a same-name group contains a NON-partial row are the declarations genuinely distinct — that group is
//! dropped as ambiguous for qualifier resolution (`skipped_ambiguous_class_name`), never guessed. Because a
//! merged controller's methods span files, each route's emit anchor (`IoProvide::file` + dedup) is the
//! METHOD's own file, not the class row's.
//!
//! ## Minimal-API routes travel through unchanged
//! The per-file pass has TWO producers (attribute-controller + minimal-API); only the attribute-controller
//! idiom carries cross-file constants. This whole-corpus pass RE-RUNS the per-file `minimal_api::extract`
//! verbatim per file (a minimal-API registration is inherently self-contained — no corpus resolution) and
//! concatenates its output, so the report is a true SUPERSET of the per-file pass and the engine can REPLACE
//! the per-file C# `http` provides wholesale (`run_csharp_provides_project_pass`).
//!
//! ## Determinism
//! An unresolvable prefix/method-path constant, or an ambiguous class/const name, SKIPS the affected route(s)
//! (never a wrong/empty prefix) and is counted — see [`CSharpProjectProvidesReport`]'s skip fields.

use std::collections::{HashMap, HashSet};

use zzop_core::{http_interface_key, IoProvide};

use crate::adapters::provides::attribute_controller::substitute_controller_token;
use crate::adapters::provides::minimal_api;

mod collect;
mod resolve;

pub use walk_entrypoint::extract_csharp_http_provides_project;

/// Result of a whole-corpus `extract_csharp_http_provides_project` run — the C# parallel of
/// `zzop_parser_java_21::project::ProjectProvidesReport`, minus Java's CE-split fields (no `extends`-chain
/// resolution in v1).
#[derive(Debug, Clone, Default)]
pub struct CSharpProjectProvidesReport {
    pub provides: Vec<IoProvide>,
    /// Controllers whose OWN routes could not be emitted because their class-level `[Route(CONST)]` prefix
    /// referenced a constant that did not resolve against the corpus (out-of-corpus, ambiguous, or computed).
    /// Counted once per blocked controller class.
    pub skipped_unresolved_prefix: u32,
    /// Simple class names declared in 2+ places in this corpus (cannot tell which declaration a qualifier
    /// reference means) — a count of the individual duplicate declarations dropped.
    pub skipped_ambiguous_class_name: u32,
    /// Routes whose own METHOD-level `[HttpGet(CONST)]`/`[Route(CONST)]` path was a non-literal constant
    /// reference that did not resolve against the corpus — the method-path analog of
    /// `skipped_unresolved_prefix`. Skipped, never keyed at the empty base.
    pub skipped_unresolved_method_path: u32,
}

/// One `class_declaration`'s structural facts, collected once per file (`collect::walk_class`) — every class
/// gets a row (a non-controller `static class Routes` holds only `constants`; a controller additionally
/// carries a `prefix` and `methods`). A C# `partial class` produces ONE row per declaring file; the assembly
/// step MERGES same-name partial rows into one (module doc's "Partial classes").
struct ClassRow {
    /// The file this row's own declaration lives in. NOT the emit anchor for a partial class's routes (whose
    /// halves span files) — each `MethodRoute` carries its own `file` for that; see `emit_controller_routes`.
    file: String,
    /// Own simple name — the `[controller]`-token substitution base, and this row's key in the corpus map.
    simple_name: String,
    is_controller: bool,
    /// `true` when the declaration carries the `partial` modifier — the signal that two same-name rows are
    /// ONE class split across files (to be merged), not two genuinely-distinct classes (to be dropped).
    is_partial: bool,
    prefix: ClassPrefix,
    /// This class's own DIRECT `const string`/`static readonly string` fields whose value is a SIMPLE string
    /// literal (a concatenated/computed initializer is deliberately not recorded — v1 scope).
    constants: HashMap<String, String>,
    /// This class's own DIRECT route methods, collected regardless of `is_controller` (a `partial` half may
    /// hold a method while the `[ApiController]` sits on another half — `collect::walk_class`). Only a row
    /// whose RESOLVED `is_controller` is true actually emits them (`emit_controller_routes`).
    methods: Vec<MethodRoute>,
}

/// A class-level `[Route]` prefix as collected per-file, before whole-corpus resolution. `Literal` (already
/// `[controller]`-substituted; the empty string for an absent `[Route]`) is emitted as-is; `NonLiteral`
/// carries the attribute's raw args so the corpus can resolve the prefix constant (`[controller]` substitution
/// then applies to the resolved value).
enum ClassPrefix {
    Literal(String),
    NonLiteral(String),
}

struct MethodRoute {
    /// The file this method is DECLARED in — the emit anchor for its `IoProvide` (and its dedup key). A
    /// merged partial controller's methods span files, so this must come from the method, not the class row.
    file: String,
    line: u32,
    symbol: Option<String>,
    verb: String,
    path: MethodPath,
}

/// A method-level route path as collected per-file. `Literal` (a quoted path, or the empty base for a bare
/// `[HttpGet]`) is emitted as-is; `NonLiteral` carries the raw args for corpus constant resolution.
enum MethodPath {
    Literal(String),
    NonLiteral(String),
}

/// Kept a private submodule solely so the `pub use` above re-exports one clean crate-root symbol without
/// leaking `mod.rs`'s internal helpers.
mod walk_entrypoint {
    use super::*;

    /// Extracts ASP.NET Core HTTP route `IoProvide`s across an entire C# corpus — see module doc. Never
    /// panics: a file that fails to parse is silently skipped (degrades to "no rows from this file").
    pub fn extract_csharp_http_provides_project(
        files: &[(String, String)],
    ) -> CSharpProjectProvidesReport {
        let mut rows_by_name: HashMap<String, Vec<ClassRow>> = HashMap::new();
        // Minimal-API routes are per-file (no corpus resolution) — collected straight into `provides`.
        let mut provides: Vec<IoProvide> = Vec::new();
        for (rel, text) in files {
            let Some(tree) = crate::parse_tree(text) else {
                continue;
            };
            collect::collect_from_root(rel, tree.root_node(), text, &mut rows_by_name);
            minimal_api::extract(rel, tree.root_node(), text, &mut provides);
        }

        // Name resolution (module doc's "Partial classes"): a name declared once is used directly; 2+ rows
        // that are ALL `partial` are one class split across files and MERGED; 2+ rows that are not all
        // partial are genuinely-distinct classes, dropped as ambiguous and counted.
        let mut skipped_ambiguous_class_name = 0u32;
        let mut classes: HashMap<String, ClassRow> = HashMap::new();
        for (name, mut rows) in rows_by_name {
            if rows.len() == 1 {
                classes.insert(name, rows.pop().unwrap());
            } else if rows.iter().all(|r| r.is_partial) {
                classes.insert(name, merge_partial_rows(rows));
            } else {
                skipped_ambiguous_class_name += rows.len() as u32;
            }
        }

        let bare = resolve::build_bare_index(&classes);
        let mut seen: HashSet<(String, String, u32, Option<String>)> = HashSet::new();
        let mut skipped_unresolved_prefix = 0u32;
        let mut skipped_unresolved_method_path = 0u32;

        // Deterministic iteration: controller names sorted, independent of `HashMap` order.
        let mut controller_names: Vec<&String> = classes
            .iter()
            .filter(|(_, row)| row.is_controller)
            .map(|(name, _)| name)
            .collect();
        controller_names.sort();

        for name in controller_names {
            emit_controller_routes(
                &classes[name],
                &classes,
                &bare,
                &mut provides,
                &mut seen,
                &mut skipped_unresolved_prefix,
                &mut skipped_unresolved_method_path,
            );
        }

        CSharpProjectProvidesReport {
            provides,
            skipped_unresolved_prefix,
            skipped_ambiguous_class_name,
            skipped_unresolved_method_path,
        }
    }

    /// Emits every `IoProvide` for one controller's routes under its resolved prefix — a non-literal prefix
    /// constant that does not resolve BLOCKS the whole class (counted once, never keyed under a wrong base);
    /// an unresolvable method-path constant skips just that route (counted). Deduped against `seen`.
    #[allow(clippy::too_many_arguments)]
    fn emit_controller_routes(
        row: &ClassRow,
        classes: &HashMap<String, ClassRow>,
        bare: &HashMap<String, Option<String>>,
        provides: &mut Vec<IoProvide>,
        seen: &mut HashSet<(String, String, u32, Option<String>)>,
        skipped_unresolved_prefix: &mut u32,
        skipped_unresolved_method_path: &mut u32,
    ) {
        let prefix = match &row.prefix {
            ClassPrefix::Literal(p) => p.clone(),
            ClassPrefix::NonLiteral(args) => match resolve::resolve_ref(args, classes, bare) {
                Some(v) => substitute_controller_token(&v, &row.simple_name),
                None => {
                    *skipped_unresolved_prefix += 1;
                    return;
                }
            },
        };
        for method in &row.methods {
            let method_path = match &method.path {
                MethodPath::Literal(p) => p.clone(),
                MethodPath::NonLiteral(args) => match resolve::resolve_ref(args, classes, bare) {
                    Some(v) => v,
                    None => {
                        *skipped_unresolved_method_path += 1;
                        continue;
                    }
                },
            };
            let full_path = format!("{prefix}/{method_path}");
            let key = http_interface_key(&method.verb, &full_path);
            // Anchor on the METHOD's own file (not `row.file`): a merged partial controller's methods span
            // files, so the provide must be keyed to where each method is actually declared.
            let dedupe_key = (
                key.clone(),
                method.file.clone(),
                method.line,
                method.symbol.clone(),
            );
            if !seen.insert(dedupe_key) {
                continue;
            }
            provides.push(IoProvide {
                body: None,
                kind: "http".to_string(),
                key,
                file: method.file.clone(),
                line: method.line,
                symbol: method.symbol.clone(),
            });
        }
    }

    /// Merges 2+ same-name `partial class` halves into ONE `ClassRow` (module doc's "Partial classes").
    /// Rows are sorted by declaring file first so the merge — first-non-empty prefix, first-wins constant on a
    /// key collision — is DETERMINISTIC regardless of `HashMap` iteration order. `methods` are concatenated in
    /// that same file order (each already carries its own `file` for emit); `is_controller` is the OR of the
    /// halves (a controller attribute on any half makes the whole class a controller).
    fn merge_partial_rows(mut rows: Vec<ClassRow>) -> ClassRow {
        rows.sort_by(|a, b| a.file.cmp(&b.file));
        let simple_name = rows[0].simple_name.clone();
        let file = rows[0].file.clone();
        let mut is_controller = false;
        let mut prefix = ClassPrefix::Literal(String::new());
        let mut prefix_fixed = false;
        let mut constants: HashMap<String, String> = HashMap::new();
        let mut methods: Vec<MethodRoute> = Vec::new();
        for row in rows {
            is_controller |= row.is_controller;
            // First non-empty prefix among the halves wins — a partial controller declares its `[Route]` on at
            // most one half in practice; a `Literal("")` (no `[Route]` on this half) does not fix the prefix.
            if !prefix_fixed && !matches!(&row.prefix, ClassPrefix::Literal(p) if p.is_empty()) {
                prefix = row.prefix;
                prefix_fixed = true;
            }
            for (name, value) in row.constants {
                constants.entry(name).or_insert(value); // first-wins on a key collision
            }
            methods.extend(row.methods);
        }
        ClassRow {
            file,
            simple_name,
            is_controller,
            is_partial: true,
            prefix,
            constants,
            methods,
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod partial_tests;
