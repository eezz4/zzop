//! Python's second hop — the additive bridge for a MODULE-attribute receiver, and why it is a bridge
//! rather than a change to the resolver.
//!
//! ## The two id spaces
//!
//! `zzop_core::callgraph::resolve`'s `resolve_method` reads an import binding as a CLASS: for
//! `from app import helpers`, the binding is `specifier = "app"`, `original = "helpers"`, so a call
//! `helpers.ensure()` mints `app/__init__.py#helpers.ensure`. In Python that binding is usually not a
//! class at all — it is a MODULE — and `ensure`'s own outgoing edges leave from `app/helpers.py#ensure`.
//! Two spellings of one symbol, joined by nothing, which is the single-hop limitation
//! [`super::python_guard::resolve_python_call_target`] documents.
//!
//! ## 📏 The measured consequence — a false finding, not a missed one
//!
//! Four fixtures, identical but for the receiver shape, `mutating-route-no-auth` with
//! `authGuardPattern: "require_"` (2026-09-08, external review round 12, review ledger V100):
//!
//! | handler calls                              | second hop                  | findings |
//! |--------------------------------------------|-----------------------------|----------|
//! | `auth.require_admin()` (module-attr, 1 hop) | —                           | 0 ✅     |
//! | `helpers.ensure()` (module-attr)            | `auth.require_admin()`      | **1 ❌** |
//! | `helpers.ensure()` (module-attr)            | `require_admin()` (direct)  | **1 ❌** |
//! | `ensure()` (direct import)                  | `require_admin()` (direct)  | 0 ✅     |
//!
//! So the axis is not hop COUNT, it is the receiver shape of the FIRST hop — and the shape that breaks
//! is the one Google's Python style guide mandates ("import modules, not names"). The route is guarded,
//! the finding says it may not be, and `--fail-on info` turns that into exit 3.
//!
//! ## Why additive, and why not fix the resolver
//!
//! `resolve_method` is language-agnostic and receives no signal that a binding is a module: the
//! `resolve_file` callback takes `(specifier, from_file)` and never sees `original`. Teaching it to try
//! `"{specifier}.{original}"` would push Python's module-path syntax into the shared path, where a
//! TypeScript `import { helpers } from './x'` would ask the same question and a class and a module
//! sharing a name could silently swap an edge's meaning.
//!
//! Bridging afterwards has neither problem, for the reason [`super::java_bridge`] gives: an edge set is a
//! `Vec`, so a bridge ADDS a path without moving or dropping the one that was there. If the receiver
//! really was a class, its original edge is still in the graph and the traversal still finds it.
//!
//! One pass reaches any depth — a bridged-to file's own outgoing edges are already in the graph, and
//! their targets are bridged in this same pass. The two-hop fixture above needs exactly that: the
//! handler's hop and the helper's hop are both module-attribute calls, and both are bridged here.

use std::collections::{HashMap, HashSet};

use zzop_core::callgraph::{SymbolEdge, SymbolGraph};

/// Bridge edges `<pkg>/__init__.py#<mod>.<name>` -> `<pkg>/<mod>.py#<name>` for every graph target whose
/// receiver names a SUBMODULE of the package the target's file is the `__init__.py` of.
///
/// The derivation needs no import table: a target file that is a package's `__init__.py` is exactly the
/// file `from <pkg> import <mod>` resolves to, so the submodule sits beside it. Both spellings are tried
/// (`<mod>.py` and `<mod>/__init__.py`).
///
/// `<name>` must be declared by the module file. That check is what keeps this from inventing symbols —
/// a resolver-minted id is a CANDIDATE, not a verified node, a contract `zzop_core::callgraph::resolve`
/// states and one consumer has already been burnt by. Without it a bridge would assert that a file
/// declares something it does not.
///
/// Returns edges sorted and deduplicated; the caller appends them to an already-deterministic graph.
pub(super) fn bridge_edges(
    symbol_graph: &SymbolGraph,
    tree_files: &HashSet<String>,
    local_symbols_by_file: &HashMap<String, HashSet<String>>,
) -> Vec<SymbolEdge> {
    let mut out: Vec<SymbolEdge> = Vec::new();
    for edge in symbol_graph {
        let Some((file, symbol)) = edge.to.split_once('#') else {
            continue;
        };
        // Only a package initializer can have submodules beside it. A target in `app/helpers.py` whose
        // receiver is `x` means `x` is an attribute of that module, not a sibling module — nothing to
        // bridge, and no file lookup to pay for.
        let Some(pkg_dir) = file.strip_suffix("/__init__.py") else {
            continue;
        };
        // `<receiver>.<method>`, the shape `resolve_method` mints for a non-namespace binding. A bare
        // member (no dot) is already the namespace shape and needs no bridge.
        let Some((receiver, method)) = symbol.split_once('.') else {
            continue;
        };
        // A dotted receiver (`app.helpers.ensure()`) is a different shape this does not claim to cover;
        // saying so here is cheaper than a reader deriving it from a silent non-match.
        if method.contains('.') {
            continue;
        }
        for candidate in [
            format!("{pkg_dir}/{receiver}.py"),
            format!("{pkg_dir}/{receiver}/__init__.py"),
        ] {
            if !tree_files.contains(&candidate) {
                continue;
            }
            if local_symbols_by_file
                .get(&candidate)
                .is_some_and(|names| names.contains(method))
            {
                out.push(SymbolEdge {
                    from: edge.to.clone(),
                    to: format!("{candidate}#{method}"),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    out.dedup_by(|a, b| a.from == b.from && a.to == b.to);
    out
}

#[cfg(test)]
mod tests;
