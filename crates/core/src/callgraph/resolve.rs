//! Resolution half of the call-graph substrate: per-file `RawCall`s -> cross-file `SymbolEdge`s, and the
//! whole-repo roll-up. The traversal half lives in [`super::bfs`]; the two share only [`SymbolGraph`].

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ir::ImportMap;

use super::{RawCall, SymbolEdge, SymbolGraph};

/// Resolves one file's `RawCall`s into `SymbolEdge`s. `resolve_file` is an injected callback that resolves
/// an import specifier to its canonical file path, or `None` for an external/unresolvable module (that
/// call is then dropped, never guessed).
///
/// Resolution rules:
/// - a method call (`RawCall::receiver_type` set): the receiver class resolves via `imports` (cross-file,
///   `<resolvedFile>#<original>.<method>`; a namespace receiver — `original == "*"` — targets the bare
///   member `<resolvedFile>#<method>`) or via `local_symbols` (same-file, `<from_file>#<receiver_type>.<method>`).
/// - a plain identifier call or heritage super name: same lookup order by `callee_name` directly
///   (`<resolvedFile>#<original>` or `<from_file>#<callee_name>`).
/// - anything resolving through neither `imports` nor `local_symbols` (implicit global, unresolvable
///   external) is dropped — this resolver never invents an edge for a name it cannot place.
pub fn resolve_calls_for_file(
    calls: &[RawCall],
    imports: &ImportMap,
    from_file: &str,
    local_symbols: &HashSet<String>,
    resolve_file: &dyn Fn(&str, &str) -> Option<String>,
) -> Vec<SymbolEdge> {
    resolve_calls_for_file_with_unresolved(calls, imports, from_file, local_symbols, resolve_file).0
}

/// [`resolve_calls_for_file`] plus the calls it DROPPED: `(from_symbol, callee_name)` for every
/// `RawCall` this resolver could not place, in input order.
///
/// The dropped set is not noise, and treating it as such produced a measured defect. A guard declared
/// inside a factory is not a top-level symbol, so a handler calling it draws no edge — and
/// `mutating-route-no-auth`, walking edges alone, reported the route as reaching no guard. That is the
/// rule stating an absence it never established: the WRITTEN NAME of the call was right there, and the
/// rule's own message quotes a name pattern as its criterion. A consumer that can judge a name (rather
/// than needing a symbol) should get to see these.
///
/// HERITAGE EDGES ARE EXCLUDED. An unresolved `extends`/`implements` name is not a call site at all, so
/// listing it here would invite a consumer to read a superclass name as something the code invokes.
///
/// One traversal produces both halves. Resolution walks an `ImportMap` and allocates a formatted id per
/// hit, so resolving each call twice — once to collect edges, once to collect the misses — would double
/// the cost of every call site in the repo to compute two views of the same answer.
pub fn resolve_calls_for_file_with_unresolved(
    calls: &[RawCall],
    imports: &ImportMap,
    from_file: &str,
    local_symbols: &HashSet<String>,
    resolve_file: &dyn Fn(&str, &str) -> Option<String>,
) -> (Vec<SymbolEdge>, Vec<(String, String)>) {
    let mut edges = Vec::new();
    let mut unresolved = Vec::new();
    for call in calls {
        match resolve_one(call, imports, from_file, local_symbols, resolve_file) {
            Some(to) => edges.push(SymbolEdge {
                from: call.from_symbol.clone(),
                to,
            }),
            // A heritage name is not something the code invokes, so it is dropped from the edge list
            // (as before) AND withheld from the unresolved list.
            None if !call.is_heritage => {
                unresolved.push((call.from_symbol.clone(), call.callee_name.clone()))
            }
            None => {}
        }
    }
    (edges, unresolved)
}

fn resolve_one(
    call: &RawCall,
    imports: &ImportMap,
    from_file: &str,
    local_symbols: &HashSet<String>,
    resolve_file: &dyn Fn(&str, &str) -> Option<String>,
) -> Option<String> {
    match &call.receiver_type {
        Some(receiver_type) => resolve_method(
            receiver_type,
            &call.callee_name,
            imports,
            from_file,
            local_symbols,
            resolve_file,
        ),
        // Heritage (super) and regular identifier calls share the same name resolution — the only
        // difference is whether the super name is imported or local.
        None => resolve_name(
            &call.callee_name,
            imports,
            from_file,
            local_symbols,
            resolve_file,
        ),
    }
}

/// Resolves the receiver class via import or local, then combines to `<classFile>#<OriginalClass>.<method>`.
///
/// **The id it returns is a CANDIDATE, not a verified node.** Nothing here checks that a `SourceSymbol`
/// with that id exists: an imported name is enough, so a Python `Annotated` alias, a TS `type` alias, or
/// any other non-class binding mints a well-formed id for a symbol that was never declared. That is
/// deliberate — a candidate graph is what lets a consumer decide how much evidence it needs — but it is
/// a contract a consumer must read, and one did not: `mutating_route_no_auth` accepted such an id's
/// QUALIFIER as auth evidence, so `session: SessionDep` + `session.add(...)` cleared an unauthenticated
/// write route (2026-07-27, fixed on the rule side by requiring the qualifier name to be a declared
/// symbol). Any consumer that reads meaning out of an id's SHAPE owes itself the same check.
/// A namespace receiver (`import * as X` / `var X = require(...)`, `original == "*"`) targets the bare
/// member `<file>#<method>` — matches how CommonJS/namespace exports are emitted as bare-member symbols.
fn resolve_method(
    receiver_type: &str,
    method: &str,
    imports: &ImportMap,
    from_file: &str,
    local_symbols: &HashSet<String>,
    resolve_file: &dyn Fn(&str, &str) -> Option<String>,
) -> Option<String> {
    if let Some(binding) = imports.get(receiver_type) {
        let file = resolve_file(&binding.specifier, from_file)?;
        return Some(if binding.original == "*" {
            format!("{file}#{method}")
        } else {
            format!("{file}#{}.{method}", binding.original)
        });
    }
    if local_symbols.contains(receiver_type) {
        return Some(format!("{from_file}#{receiver_type}.{method}"));
    }
    None
}

/// Resolves an identifier name: imported -> `<file>#<original>`; same-file declaration -> `<fromFile>#<name>`.
fn resolve_name(
    name: &str,
    imports: &ImportMap,
    from_file: &str,
    local_symbols: &HashSet<String>,
    resolve_file: &dyn Fn(&str, &str) -> Option<String>,
) -> Option<String> {
    if let Some(binding) = imports.get(name) {
        let file = resolve_file(&binding.specifier, from_file)?;
        return Some(format!("{file}#{}", binding.original));
    }
    if local_symbols.contains(name) {
        return Some(format!("{from_file}#{name}"));
    }
    None
}

/// Builds the whole-repo `SymbolGraph` from every file's `RawCall`s — groups `calls` by the file segment of `RawCall::from_symbol`
/// (`"<file>#<name>"`, split at the first `#`) and resolves each file's group with that file's own
/// `ImportMap`/local-symbol set. A file with no entry in `imports_by_file`/`local_symbols_by_file` resolves
/// as if both were empty (no imports, no local symbols) rather than panicking.
pub fn build_symbol_graph(
    calls: &[RawCall],
    imports_by_file: &HashMap<String, ImportMap>,
    local_symbols_by_file: &HashMap<String, HashSet<String>>,
    resolve_file: &dyn Fn(&str, &str) -> Option<String>,
) -> SymbolGraph {
    build_symbol_graph_with_unresolved(calls, imports_by_file, local_symbols_by_file, resolve_file)
        .0
}

/// [`build_symbol_graph`] plus the whole run's dropped calls, indexed by CALLER symbol —
/// `from_symbol -> callee names this resolver could not place`, deduplicated and sorted per caller so
/// the map is deterministic. See [`resolve_calls_for_file_with_unresolved`] for why the dropped set is
/// worth carrying, and for the heritage exclusion.
pub fn build_symbol_graph_with_unresolved(
    calls: &[RawCall],
    imports_by_file: &HashMap<String, ImportMap>,
    local_symbols_by_file: &HashMap<String, HashSet<String>>,
    resolve_file: &dyn Fn(&str, &str) -> Option<String>,
) -> (SymbolGraph, BTreeMap<String, Vec<String>>) {
    let mut by_file: BTreeMap<&str, Vec<RawCall>> = BTreeMap::new();
    for call in calls {
        let file = call
            .from_symbol
            .split('#')
            .next()
            .unwrap_or(call.from_symbol.as_str());
        by_file.entry(file).or_default().push(call.clone());
    }
    let empty_imports = ImportMap::new();
    let empty_locals: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let mut unresolved: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (file, file_calls) in by_file {
        let imports = imports_by_file.get(file).unwrap_or(&empty_imports);
        let locals = local_symbols_by_file.get(file).unwrap_or(&empty_locals);
        let (edges, dropped) = resolve_calls_for_file_with_unresolved(
            &file_calls,
            imports,
            file,
            locals,
            resolve_file,
        );
        out.extend(edges);
        for (from, callee) in dropped {
            unresolved.entry(from).or_default().push(callee);
        }
    }
    for names in unresolved.values_mut() {
        names.sort_unstable();
        names.dedup();
    }
    (out, unresolved)
}
