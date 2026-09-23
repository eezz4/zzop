//! The GRAPH-BUILDING half of `super::run_callgraph_rules` — four languages' re-parses, the combined
//! per-language resolver, and the one `SymbolGraph` every rule in the pass then reads.
//!
//! Split out of the parent for size, but the seam is real: everything here answers *what does this tree
//! call*, and nothing here knows which rules are on. The parent keeps the gating and the rules; this
//! module keeps the substrate. The per-language docs stay with their own modules
//! (`java_guard`/`python_guard`/`rust_guard`/`java_bridge`/`python_bridge`); what lives here is the ORDER they run in and
//! the memory contract the TS loop states.

use std::collections::{HashMap, HashSet};

use zzop_core::callgraph::{RawCall, SymbolGraph};
use zzop_core::ImportMap;

use super::{decorator_gate, java_bridge, java_guard, python_bridge, python_guard, rust_guard};

/// Everything the rules half of the pass reads. Grouped rather than returned as a tuple because five of
/// the six fields are consumed by DIFFERENT rules, and a tuple would make each call site re-state which
/// position meant what.
pub(super) struct BuiltGraph {
    pub(super) raw_calls: Vec<RawCall>,
    pub(super) symbol_graph: SymbolGraph,
    /// Calls the resolver could not place, by caller — guard evidence in its own right; see
    /// `zzop_rules_http::ScanMutatingRouteNoAuthInput::unresolved_callees`.
    pub(super) unresolved_callees: std::collections::BTreeMap<String, Vec<String>>,
    pub(super) ts_guards: decorator_gate::TsGuardEvidence,
    pub(super) java: java_guard::JavaGuards,
    pub(super) python_guards: python_guard::PythonGuards,
}

/// Reads and re-parses the tree's TS/Java/Python/Rust sources, resolves every call into one
/// [`SymbolGraph`], and bridges the two-node-id-space languages on top of it — Java and Python, each
/// via its own `*_bridge` below. (Said "Java's" alone until 2026-09-12, four days after the Python
/// bridge landed; review ledger V169.)
#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    root: &std::path::Path,
    ts_paths: &HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    ts_call_graph_pairs: &[(String, zzop_core::callgraph::CallGraphFacts)],
    java_rels: &[String],
    java_index: &crate::pipeline::JavaIndex,
    rust_workspace: &crate::pipeline::RustWorkspaceMap,
    all_symbols: &[zzop_core::SourceSymbol],
    vocab: &crate::vocabulary::ResolvedVocabulary<'_>,
    need_decorator_guarded: bool,
) -> BuiltGraph {
    // Where these came from, and why not from here.
    //
    // This function used to open every `ts_paths` entry, read it, and hand the text to
    // `parse_calls` plus three NestJS guard extractors. That was a SECOND full swc parse of every
    // `.ts` file in the tree: `parse_with_cm`'s memo is one entry and thread-local, so it collapses
    // the per-file lane's consecutive extractors and is never warm for a pass that runs afterwards on
    // other threads. Measured on this repository: 68% of a warm run (~7.5s of 11.4s), and exactly 2N
    // parses for an N-file tree — `analyze_parse_census` holds that number as a pin. Review ledger V108.
    //
    // The extraction now happens in `pipeline::fresh::call_graph`, off the parse that lane already
    // paid for, and rides the per-file cache — so a warm file pays neither the read nor the parse. The
    // memory contract the old loop stated (each file's text dropped at the end of its own iteration)
    // is kept by construction: no text reaches this function at all any more, only the small facts.
    //
    // ⚠ The `need_decorator_guarded` gate did NOT move with the extractors. It still decides whether
    // the guard evidence is READ (below, and at this function's callers); what it no longer does is
    // decide whether it is produced. Gating production would have kept the read and the parse for
    // every tree with an auth rule enabled, which is most of them, and made the halved parse count a
    // property of the fixture rather than of the engine.
    let mut raw_calls: Vec<RawCall> = Vec::new();
    let mut ts_guards = decorator_gate::TsGuardEvidence::default();
    for (rel, facts) in ts_call_graph_pairs {
        raw_calls.extend(facts.raw_calls.iter().cloned());
        if need_decorator_guarded {
            ts_guards.absorb(rel, facts);
        }
    }
    let mut imports_by_file: HashMap<String, ImportMap> = ts_import_pairs.iter().cloned().collect();
    // Java's own re-parse — module doc "Engine-wiring route taken"; `java_guard`'s own doc for why its
    // imports are parsed fresh here and its text is not retained past its own iteration, the same
    // memory contract the TS loop above states.
    let java = java_guard::parse_calls_and_guards(
        root,
        java_rels,
        need_decorator_guarded,
        &mut raw_calls,
        &mut imports_by_file,
    );
    // Python's own re-parse + its two decorator-guard producers — module doc "Engine-wiring route taken",
    // and `python_guard`'s own doc for the two guard shapes and why they are gathered in two phases.
    let python_guards = python_guard::parse_calls_and_guards(
        root,
        ts_paths,
        need_decorator_guarded,
        &vocab.python_guard(),
        &mut raw_calls,
    );
    // Rust's extraction is NOT here any more (2026-09-08, review ledger V111). It used to re-read and
    // re-parse every `.rs` file with `syn` on this line, and splitting the pass's own wall clock
    // said that single call was **4.4-5.8s of a ~7.5s warm run on this repository** — the largest term
    // in the pass by a wide margin, and the one nobody had named. Its facts now ride
    // `ts_call_graph_pairs` beside TypeScript's, off the parse the per-file lane already paid for.
    // What stays here is RESOLUTION (`resolve_rust_call_target` below), which needs the workspace map
    // and the whole tree's path set and so cannot move.
    let mut local_symbols_by_file: HashMap<String, HashSet<String>> = HashMap::new();
    for s in all_symbols {
        local_symbols_by_file
            .entry(s.file.clone())
            .or_default()
            .insert(s.name.clone());
    }
    // Combined resolver, dispatched by the CALLING file's own extension — module doc "Java call
    // resolution".
    let py_roots = &vocab.python_package_roots;
    let resolve_file_fn = |specifier: &str, from_file: &str| {
        if from_file.ends_with(".java") {
            Some(specifier.to_string())
        } else if crate::analyze::assemble::helpers::is_python_source_ext(from_file) {
            python_guard::resolve_python_call_target(specifier, from_file, ts_paths, py_roots)
        } else if crate::analyze::assemble::helpers::is_rust_source_ext(from_file) {
            rust_guard::resolve_rust_call_target(specifier, from_file, ts_paths, rust_workspace)
        } else {
            zzop_parser_typescript::resolve_file(specifier, from_file, ts_paths)
        }
    };
    // Both halves: the resolved edges, and the calls the resolver DROPPED indexed by caller. A dropped
    // name is still guard evidence for `mutating-route-no-auth` (see its `unresolved_callees` field) —
    // a guard the resolver cannot place is a call whose written name the rule can read.
    let (mut symbol_graph, unresolved_callees) =
        zzop_core::callgraph::build_symbol_graph_with_unresolved(
            &raw_calls,
            &imports_by_file,
            &local_symbols_by_file,
            &resolve_file_fn,
        );
    // Java's second hop. The resolver above cannot draw it (a specifier resolves to itself, so a
    // cross-file Java edge lands on a node the callee file leaves from under a DIFFERENT spelling), and
    // it cannot be fixed inside the resolver because one wildcard specifier resolves to many files while
    // `resolve_file_fn` returns one target. Additive: no existing edge is moved or dropped.
    symbol_graph.extend(java_bridge::bridge_edges(
        &symbol_graph,
        java_index,
        &local_symbols_by_file,
    ));
    // Python's second hop, same shape and same reason. `resolve_method` reads an import binding as a
    // CLASS, so `from app import helpers` + `helpers.ensure()` lands on
    // `app/__init__.py#helpers.ensure` while that function's own edges leave from
    // `app/helpers.py#ensure`. Measured consequence: a FALSE `mutating-route-no-auth` on a guarded
    // route whose handler calls the guard through a module attribute — the import style Google's
    // Python guide mandates. Additive, so a receiver that really was a class keeps its edge.
    // Review ledger V100.
    symbol_graph.extend(python_bridge::bridge_edges(
        &symbol_graph,
        ts_paths,
        &local_symbols_by_file,
    ));
    BuiltGraph {
        raw_calls,
        symbol_graph,
        unresolved_callees,
        ts_guards,
        java,
        python_guards,
    }
}
