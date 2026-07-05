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
/// a parser (`zpz_parser_typescript::calls::parse_calls`); cross-file resolution into a `SymbolEdge` is
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
mod tests {
    //! Exercises `resolve_calls_for_file`'s resolution rules end-to-end, plus unit tests for
    //! `build_symbol_graph`/`bfs_depths`/`bfs_reachable` over this module's `SymbolGraph` shape.
    use super::*;
    use crate::ir::ImportBinding;

    fn call(from_symbol: &str, callee_name: &str, line: u32) -> RawCall {
        RawCall {
            from_symbol: from_symbol.to_string(),
            callee_name: callee_name.to_string(),
            line,
            receiver_type: None,
            is_heritage: false,
        }
    }

    fn method_call(
        from_symbol: &str,
        callee_name: &str,
        line: u32,
        receiver_type: &str,
    ) -> RawCall {
        RawCall {
            receiver_type: Some(receiver_type.to_string()),
            ..call(from_symbol, callee_name, line)
        }
    }

    fn heritage_call(from_symbol: &str, callee_name: &str, line: u32) -> RawCall {
        RawCall {
            is_heritage: true,
            ..call(from_symbol, callee_name, line)
        }
    }

    fn binding(specifier: &str, original: &str) -> ImportBinding {
        ImportBinding {
            specifier: specifier.to_string(),
            original: original.to_string(),
            deferred: false,
            type_only: false,
        }
    }

    fn resolve_only<'a>(
        specifier: &'a str,
        target: &'a str,
    ) -> impl Fn(&str, &str) -> Option<String> + 'a {
        move |s, _from| (s == specifier).then(|| target.to_string())
    }

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    // --- resolve_calls_for_file ---

    #[test]
    fn imported_call_resolves_to_resolved_file_hash_original() {
        let mut imports = ImportMap::new();
        imports.insert("bar".to_string(), binding("./b", "bar"));
        let out = resolve_calls_for_file(
            &[call("a.ts#foo", "bar", 3)],
            &imports,
            "a.ts",
            &HashSet::new(),
            &resolve_only("./b", "b.ts"),
        );
        assert_eq!(
            out,
            vec![SymbolEdge {
                from: "a.ts#foo".to_string(),
                to: "b.ts#bar".to_string()
            }]
        );
    }

    #[test]
    fn same_file_local_call_resolves_to_from_file_hash_name() {
        let out = resolve_calls_for_file(
            &[call("a.ts#foo", "local", 2)],
            &ImportMap::new(),
            "a.ts",
            &set(&["local"]),
            &|_, _| None,
        );
        assert_eq!(
            out,
            vec![SymbolEdge {
                from: "a.ts#foo".to_string(),
                to: "a.ts#local".to_string()
            }]
        );
    }

    #[test]
    fn aliased_import_looks_up_by_alias_key_to_uses_original_name() {
        let mut imports = ImportMap::new();
        imports.insert("X".to_string(), binding("./b", "RealX"));
        let out = resolve_calls_for_file(
            &[call("a.ts#foo", "X", 3)],
            &imports,
            "a.ts",
            &HashSet::new(),
            &resolve_only("./b", "b.ts"),
        );
        assert_eq!(out[0].to, "b.ts#RealX");
    }

    #[test]
    fn external_module_unresolvable_specifier_is_dropped() {
        let mut imports = ImportMap::new();
        imports.insert("useState".to_string(), binding("react", "useState"));
        let out = resolve_calls_for_file(
            &[call("a.ts#foo", "useState", 3)],
            &imports,
            "a.ts",
            &HashSet::new(),
            &|_, _| None,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn identifier_neither_import_nor_local_is_dropped() {
        let out = resolve_calls_for_file(
            &[call("a.ts#foo", "structuredClone", 3)],
            &ImportMap::new(),
            "a.ts",
            &HashSet::new(),
            &|_, _| None,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn cross_file_method_receiver_type_imported_class() {
        let mut imports = ImportMap::new();
        imports.insert("Svc".to_string(), binding("./svc", "Svc"));
        let out = resolve_calls_for_file(
            &[method_call("a.ts#foo", "run", 3, "Svc")],
            &imports,
            "a.ts",
            &HashSet::new(),
            &resolve_only("./svc", "svc.ts"),
        );
        assert_eq!(
            out,
            vec![SymbolEdge {
                from: "a.ts#foo".to_string(),
                to: "svc.ts#Svc.run".to_string()
            }]
        );
    }

    #[test]
    fn cross_file_method_receiver_class_declared_in_same_file() {
        let out = resolve_calls_for_file(
            &[method_call("a.ts#foo", "run", 3, "Local")],
            &ImportMap::new(),
            "a.ts",
            &set(&["Local"]),
            &|_, _| None,
        );
        assert_eq!(
            out,
            vec![SymbolEdge {
                from: "a.ts#foo".to_string(),
                to: "a.ts#Local.run".to_string()
            }]
        );
    }

    #[test]
    fn cross_file_method_receiver_class_from_external_module_is_dropped() {
        let mut imports = ImportMap::new();
        imports.insert("Logger".to_string(), binding("winston", "Logger"));
        let out = resolve_calls_for_file(
            &[method_call("a.ts#foo", "log", 3, "Logger")],
            &imports,
            "a.ts",
            &HashSet::new(),
            &|_, _| None,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn heritage_extends_imported_super_resolves_class_to_super_edge() {
        let mut imports = ImportMap::new();
        imports.insert("Base".to_string(), binding("./base", "Base"));
        let out = resolve_calls_for_file(
            &[heritage_call("a.ts#Child", "Base", 1)],
            &imports,
            "a.ts",
            &HashSet::new(),
            &resolve_only("./base", "base.ts"),
        );
        assert_eq!(
            out,
            vec![SymbolEdge {
                from: "a.ts#Child".to_string(),
                to: "base.ts#Base".to_string()
            }]
        );
    }

    #[test]
    fn heritage_same_file_super() {
        let out = resolve_calls_for_file(
            &[heritage_call("a.ts#Child", "Base", 1)],
            &ImportMap::new(),
            "a.ts",
            &set(&["Base"]),
            &|_, _| None,
        );
        assert_eq!(
            out,
            vec![SymbolEdge {
                from: "a.ts#Child".to_string(),
                to: "a.ts#Base".to_string()
            }]
        );
    }

    #[test]
    fn namespace_receiver_resolves_method_to_bare_member() {
        let mut imports = ImportMap::new();
        imports.insert("Body".to_string(), binding("../body/Body", "*"));
        let out = resolve_calls_for_file(
            &[method_call(
                "core/Sleeping.js#update",
                "getSpeed",
                5,
                "Body",
            )],
            &imports,
            "core/Sleeping.js",
            &HashSet::new(),
            &resolve_only("../body/Body", "body/Body.js"),
        );
        assert_eq!(
            out,
            vec![SymbolEdge {
                from: "core/Sleeping.js#update".to_string(),
                to: "body/Body.js#getSpeed".to_string()
            }]
        );
    }

    #[test]
    fn named_class_receiver_still_resolves_to_class_dot_method() {
        let mut imports = ImportMap::new();
        imports.insert("Svc".to_string(), binding("./svc", "Service"));
        let out = resolve_calls_for_file(
            &[method_call("a.ts#foo", "run", 2, "Svc")],
            &imports,
            "a.ts",
            &HashSet::new(),
            &resolve_only("./svc", "svc.ts"),
        );
        assert_eq!(out[0].to, "svc.ts#Service.run");
    }

    /// `BTreeMap::get` has no prototype chain (unlike a plain JS object) — a call name colliding with a
    /// method name on `HashMap`/`BTreeMap` itself cannot spuriously match. This test exists as a marker
    /// for that fact: it guards against a JS-only prototype-chain footgun that has no equivalent in Rust.
    #[test]
    fn call_name_colliding_with_a_map_method_name_is_not_a_false_import_binding() {
        let out = resolve_calls_for_file(
            &[call("a.ts#foo", "constructor", 2)],
            &ImportMap::new(),
            "a.ts",
            &HashSet::new(),
            &|_, _| panic!("resolve_file must not be called for an unresolved name"),
        );
        assert!(out.is_empty());
    }

    // --- build_symbol_graph ---

    #[test]
    fn build_symbol_graph_groups_calls_by_file_and_resolves_each_with_its_own_imports() {
        let mut imports_by_file = HashMap::new();
        let mut a_imports = ImportMap::new();
        a_imports.insert("helper".to_string(), binding("./b", "helper"));
        imports_by_file.insert("a.ts".to_string(), a_imports);

        let mut locals_by_file = HashMap::new();
        locals_by_file.insert("b.ts".to_string(), set(&["helper", "other"]));

        let calls = vec![
            call("a.ts#main", "helper", 1),
            call("b.ts#helper", "other", 2),
        ];
        let graph = build_symbol_graph(
            &calls,
            &imports_by_file,
            &locals_by_file,
            &resolve_only("./b", "b.ts"),
        );
        assert_eq!(graph.len(), 2);
        assert!(graph.contains(&SymbolEdge {
            from: "a.ts#main".to_string(),
            to: "b.ts#helper".to_string()
        }));
        assert!(graph.contains(&SymbolEdge {
            from: "b.ts#helper".to_string(),
            to: "b.ts#other".to_string()
        }));
    }

    #[test]
    fn build_symbol_graph_missing_file_entries_resolve_as_empty_not_panic() {
        let calls = vec![call("a.ts#main", "unknown", 1)];
        let graph = build_symbol_graph(&calls, &HashMap::new(), &HashMap::new(), &|_, _| None);
        assert!(graph.is_empty());
    }

    // --- bfs_depths / bfs_reachable ---

    fn edge(from: &str, to: &str) -> SymbolEdge {
        SymbolEdge {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    #[test]
    fn bfs_depths_source_is_depth_zero() {
        let graph = vec![edge("a", "b")];
        let depths = bfs_depths(&graph, "a");
        assert_eq!(depths.get("a"), Some(&0));
        assert_eq!(depths.get("b"), Some(&1));
    }

    #[test]
    fn bfs_depths_multi_hop_chain() {
        let graph = vec![edge("a", "b"), edge("b", "c"), edge("c", "d")];
        let depths = bfs_depths(&graph, "a");
        assert_eq!(depths.get("d"), Some(&3));
    }

    #[test]
    fn bfs_depths_unreachable_node_is_absent() {
        let graph = vec![edge("a", "b"), edge("x", "y")];
        let depths = bfs_depths(&graph, "a");
        assert!(!depths.contains_key("y"));
    }

    #[test]
    fn bfs_depths_diamond_takes_shortest_path() {
        // a -> b -> d (depth 2), a -> c -> c2 -> d (depth 3): the direct-via-b path wins.
        let graph = vec![
            edge("a", "b"),
            edge("a", "c"),
            edge("b", "d"),
            edge("c", "c2"),
            edge("c2", "d"),
        ];
        let depths = bfs_depths(&graph, "a");
        assert_eq!(depths.get("d"), Some(&2));
    }

    #[test]
    fn bfs_depths_cycle_does_not_loop_forever() {
        let graph = vec![edge("a", "b"), edge("b", "a")];
        let depths = bfs_depths(&graph, "a");
        assert_eq!(depths.len(), 2);
        assert_eq!(depths.get("a"), Some(&0));
        assert_eq!(depths.get("b"), Some(&1));
    }

    #[test]
    fn bfs_reachable_finds_closest_predicate_match() {
        let graph = vec![edge("h", "a"), edge("a", "write1"), edge("h", "write2")];
        let found = bfs_reachable(&graph, "h", |id| id.starts_with("write"));
        // "write2" is depth 1 (direct from h); "write1" is depth 2 (via a) — closest wins.
        assert_eq!(found, Some(("write2".to_string(), 1)));
    }

    #[test]
    fn bfs_reachable_ties_break_on_id_ascending() {
        let graph = vec![edge("h", "write-b"), edge("h", "write-a")];
        let found = bfs_reachable(&graph, "h", |id| id.starts_with("write"));
        assert_eq!(found, Some(("write-a".to_string(), 1)));
    }

    #[test]
    fn bfs_reachable_none_when_no_match() {
        let graph = vec![edge("h", "a")];
        assert_eq!(bfs_reachable(&graph, "h", |id| id == "nope"), None);
    }

    #[test]
    fn bfs_reachable_can_match_the_start_node_itself_at_depth_zero() {
        let graph = vec![edge("h", "a")];
        let found = bfs_reachable(&graph, "h", |id| id == "h");
        assert_eq!(found, Some(("h".to_string(), 0)));
    }
}
