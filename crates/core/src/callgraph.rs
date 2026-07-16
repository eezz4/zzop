//! Call-graph substrate — `RawCall` (parser-projected intra-file call sites) -> `SymbolEdge`/`SymbolGraph`
//! (cross-file-resolved caller-symbol -> callee-symbol edges) -> BFS reachability over that graph.
//! Downstream direction only — the only direction the call-graph rules in `rules/native/rules-graph` need.
//!
//! Backs the `rules/native/rules-graph` HTTP-handler-reachability rules (`scanUnsafeReadEndpoint` /
//! `scanNonIdempotentWrite`, both BFS-over-`symbolEdges` from an HTTP handler symbol to a store-write call).

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ir::ImportMap;

/// A single call site inside one file, attributed to its enclosing top-level symbol. Produced per-file by
/// a parser (`zzop_parser_typescript::calls::parse_calls`); cross-file resolution into a `SymbolEdge` is
/// this module's job (`resolve_calls_for_file`), not the parser's — a deliberate per-file (name-only) /
/// cross-file (ImportMap-aware) split.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawCall {
    /// Symbol id of the call site (`"x.ts#foo"`); for a heritage edge, the class symbol id.
    pub from_symbol: String,
    /// Target identifier name — unresolved for cross-file calls; for heritage, the super/interface name.
    pub callee_name: String,
    /// Call line (1-based).
    pub line: u32,
    /// For `recv.method()`, the class name of `recv` when it is a typed/imported class receiver (`new X()`,
    /// `: X` annotation) — lets `resolve_calls_for_file` emit a cross-file `<file>#<Class>.<method>` edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_type: Option<String>,
    /// True for a class `extends`/`implements` edge — `callee_name` is the super class or interface name.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_heritage: bool,
}

/// A resolved caller-symbol -> callee-symbol edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolEdge {
    pub from: String,
    pub to: String,
}

/// The whole-repo symbol call graph: a flat edge list. The BFS helpers below build their adjacency index
/// from this on demand.
pub type SymbolGraph = Vec<SymbolEdge>;

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
    calls
        .iter()
        .filter_map(|call| {
            resolve_one(call, imports, from_file, local_symbols, resolve_file).map(|to| {
                SymbolEdge {
                    from: call.from_symbol.clone(),
                    to,
                }
            })
        })
        .collect()
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
    for (file, file_calls) in by_file {
        let imports = imports_by_file.get(file).unwrap_or(&empty_imports);
        let locals = local_symbols_by_file.get(file).unwrap_or(&empty_locals);
        out.extend(resolve_calls_for_file(
            &file_calls,
            imports,
            file,
            locals,
            resolve_file,
        ));
    }
    out
}

/// Downstream-only BFS depth map from `start` over `graph` (the only direction the two call-graph rules
/// need; nodeId -> depth, 0 = `start`). Unreachable nodes are simply absent from the map.
pub fn bfs_depths(graph: &SymbolGraph, start: &str) -> BTreeMap<String, u32> {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in graph {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }

    let mut depth_by_node: BTreeMap<String, u32> = BTreeMap::new();
    depth_by_node.insert(start.to_string(), 0);
    let mut frontier: Vec<String> = vec![start.to_string()];
    let mut depth = 0u32;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for node in &frontier {
            let Some(neighbors) = adjacency.get(node.as_str()) else {
                continue;
            };
            for &neighbor in neighbors {
                if !depth_by_node.contains_key(neighbor) {
                    depth_by_node.insert(neighbor.to_string(), depth + 1);
                    next.push(neighbor.to_string());
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
        depth += 1;
    }
    depth_by_node
}

/// The reachable node (downstream of `start`, `bfs_depths` semantics) with the lowest depth for which
/// `predicate` holds, tie-broken by symbol id ascending for determinism (this crate's general convention —
/// see `registry::merge_findings`'s explicit tie-breaks — since `bfs_depths`' `BTreeMap` iteration order is
/// already id-sorted, not BFS-discovery order). Returns `None` when no reachable node (including `start`
/// itself) satisfies `predicate`. This is the shared "closest reached site wins" primitive behind
/// `scanUnsafeReadEndpoint` / `scanNonIdempotentWrite`.
pub fn bfs_reachable(
    graph: &SymbolGraph,
    start: &str,
    predicate: impl Fn(&str) -> bool,
) -> Option<(String, u32)> {
    bfs_depths(graph, start)
        .into_iter()
        .filter(|(id, _)| predicate(id))
        .min_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
}

#[cfg(test)]
mod tests;
