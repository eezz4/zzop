//! Project-wide (whole-corpus) Spring HTTP route PROVIDES resolution — the AST-grade equivalent of the
//! old lexical `zzop_parser_java::project` module, a superset of `provides::extract_http_provides`'s
//! per-file pass resolving two facts invisible to a single file:
//!
//! 1. **Non-literal class-level prefixes**: `@RequestMapping(Path.ASSET_PATH)` where `Path.ASSET_PATH`
//!    is a `public static final String` in another file, possibly a `+`-concatenation of further
//!    constants. Resolution is SCOPED TO THE DECLARING CLASS'S OWN `extends` chain, not name-global (see
//!    `resolve::resolve_class_prefix`) — an unresolvable term is counted, never guessed.
//! 2. **CE-style base-class routing**: `FooController extends FooControllerCE`, where the real mapping
//!    methods and prefix live on `FooControllerCE` (no `@RestController` of its own) while
//!    `FooController` — usually a different file — carries `@RestController` but declares no methods.
//!    Resolved here via a corpus-wide `extends`-chain walk (`walk::walk_chain`).
//!
//! Both facts are whole-project, not per-file — this module is a standalone whole-corpus pass callers
//! run explicitly, same as the old crate's `extract_http_provides_project`'s calling convention (this
//! crate's own `extract_http_provides_project` matches that signature exactly, so the wiring batch's
//! `zzop-engine` call sites — `run_java_provides_project_pass`, the `java_provides_project` example —
//! need no shape change, only the crate-name swap).
//!
//! ## AST-native design change from the old lexical crate (documented, not a parity requirement)
//! The old crate collected a flat per-file `SourceSymbol` list, then re-searched for each method's
//! "smallest containing span" class at EMISSION time. This crate instead computes each class's own
//! `(verb, path)` method routes ONCE, directly during the single per-file AST collection pass
//! (`collect::walk_class`) — a method's route depends only on its OWN annotations, never on which class
//! ends up gating it, so precomputing is behaviorally equivalent and removes the need to keep every
//! file's line/symbol data around for a second pass.
//!
//! Also, unlike the old crate's documented "a nested class's own fields are not excluded from its
//! enclosing class's constant scan" limit, this crate's constant collection is properly scoped to a
//! class's OWN direct members — a nested class's `static final String` fields no longer leak into its
//! enclosing class's constant map (a precision gain the AST's real nesting structure makes free).
//!
//! ## Determinism
//! An unresolvable prefix or ambiguous class name SKIPS every route needing it (never a wrong/empty
//! prefix) — see `ProjectProvidesReport::skipped_unresolved_prefix` / `skipped_ambiguous_class_name`.
//!
//! ## Known limits (v1 scope, same as the old crate)
//! Resolution is by SIMPLE class name only (no package/import qualification); interfaces are not walked
//! in `extends` chains (only `class_declaration` carries a `superclass` field at all).

use std::collections::HashMap;

use zzop_core::IoProvide;

mod collect;
mod resolve;
mod walk;

pub use walk::extract_http_provides_project;

/// Result of a whole-corpus `extract_http_provides_project` run — see module doc's "Determinism"
/// section. Identical shape to the old lexical crate's `ProjectProvidesReport`.
#[derive(Debug, Clone, Default)]
pub struct ProjectProvidesReport {
    pub provides: Vec<IoProvide>,
    /// Classes whose routes could not be emitted because a class-level `@RequestMapping` in their
    /// `extends` chain referenced a constant, or a qualifier/`extends` class, that did not resolve.
    pub skipped_unresolved_prefix: u32,
    /// Simple class names declared in 2+ files in this corpus (cannot safely tell which declaration an
    /// `extends`/qualifier reference means) — a count of the individual duplicate declarations skipped.
    pub skipped_ambiguous_class_name: u32,
}

/// One class/interface/enum/record/annotation-type declaration's structural facts, collected once per
/// file (`collect::walk_class`) — the AST-native counterpart of the old crate's identically-named
/// `ClassRow`, with `methods` PRECOMPUTED instead of re-derived at emission time (module doc).
struct ClassRow {
    file: String,
    /// Simple superclass name (`class_declaration` only — module doc's "known limits").
    extends: Option<String>,
    is_controller: bool,
    /// Raw argument text of this row's own `@RequestMapping`, if present — see
    /// `provides::annotations::ClassAnnotationFacts::request_mapping_arg`'s identical doc.
    request_mapping_arg: Option<String>,
    /// This row's own DIRECT `static final String NAME = <raw RHS expression>;` (or implicitly-const
    /// interface/annotation-type `String NAME = ...;`) declarations, unevaluated — see
    /// `resolve::eval_concat_expr` for how a raw expression becomes a literal value.
    constants: HashMap<String, String>,
    /// This row's own DIRECT method/constructor routes, already resolved via `provides::method_route` —
    /// module doc's "AST-native design change".
    methods: Vec<MethodRoute>,
}

struct MethodRoute {
    line: u32,
    name: String,
    verb: String,
    path: String,
}

/// A class-level `@RequestMapping` prefix's resolution state — module doc's "Determinism" section.
enum PrefixState {
    /// No `@RequestMapping` on this row at all — contributes no prefix, but does not block a search for
    /// one further down the chain.
    NoMapping,
    Resolved(String),
    /// An `@RequestMapping` is present but its argument did not resolve to a literal value.
    Unresolved,
}

#[cfg(test)]
mod tests;
