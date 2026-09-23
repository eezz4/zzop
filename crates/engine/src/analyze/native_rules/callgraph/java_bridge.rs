//! Java's second hop — the additive bridge between the two node-id spaces a Java call graph ends up
//! with, and why it is a bridge rather than a fix to the resolver.
//!
//! ## The two id spaces
//! Java import specifiers are dotted package/class names (`com.ex.svc.OrderHelper`), not paths, so the
//! combined `resolve_file_fn` in the parent module resolves a Java specifier to ITSELF. Every cross-file
//! Java edge therefore LANDS on a node named `<specifier>#<Class>.<method>`, while the callee file's own
//! outgoing edges LEAVE from `<real/path/Class.java>#<Class>.<method>`. Two spellings of one symbol, and
//! nothing joins them — which is exactly the single-hop limitation the parent module doc names.
//!
//! 📏 Measured consequence (2026-09-07, review ledger V30): the limitation is not "a guard we miss", it
//! is a FALSE FINDING. A handler -> helper (another file) -> guard chain has its second hop invisible, so
//! `mutating-route-no-auth` reports the route as reaching no guard and FIRES. A three-controller fixture
//! (hop-0 unguarded, hop-1 handler calls the guard directly, hop-2 through a helper) fired on hop-0 AND
//! hop-2; only hop-0 is real.
//!
//! ## Why additive, and why not just resolve to the file
//! The obvious fix — have the Java arm of `resolve_file_fn` return the real path — is ruled out by that
//! callback's SHAPE, not by its callers: it is `Fn(&str, &str) -> Option<String>`, ONE target, while a
//! Java wildcard import (`com.ex.svc.*`) resolves to MANY files. A one-target resolver cannot answer that
//! without lying about which file it picked. Bridging afterwards can, because an edge set is a `Vec`: one
//! specifier node may bridge to several files and the BFS decides.
//!
//! ⚠ A second objection was raised against the path-id swap and it does NOT hold, which is worth writing
//! down so nobody re-derives it: the worry was that a path id would feed `<...>/A.java#checkUser` to the
//! guard vocabulary, whose qualifier arm would then read `java`. Java symbol names are already qualified
//! (`Class.method`), so BOTH spellings end in `#AuthorizationService.checkUser` and both hand the
//! vocabulary the same qualifier. The wildcard shape above is the whole reason for going additive.
//!
//! ## Why it walks edge TARGETS rather than imports
//! The alternative was to bridge every import specifier against every symbol of the file it resolves to,
//! which mints edges for names nobody calls. Walking the already-built graph's targets mints exactly the
//! bridges the traversal can use — and one pass is enough for ANY depth, because a bridged-to file's own
//! outgoing edges are already in the graph and their targets are themselves bridged in the same pass.

use std::collections::{HashMap, HashSet};

use zzop_core::callgraph::{SymbolEdge, SymbolGraph};

use crate::analyze::assemble::helpers::resolve_java_import;
use crate::pipeline::JavaIndex;

/// Bridge edges `<specifier>#<name> -> <file>#<name>` for every graph target whose file part is a Java
/// import specifier that [`resolve_java_import`] places, and whose `<name>` that file actually declares.
///
/// The `<name>` check is what keeps this from inventing symbols: an id minted by the resolver is a
/// CANDIDATE, not a verified node (`zzop_core::callgraph::resolve` documents that contract), so bridging
/// without it would assert that a file declares something it does not. Returns edges sorted and
/// deduplicated — the caller appends them to a graph whose own order is already deterministic.
pub(super) fn bridge_edges(
    symbol_graph: &SymbolGraph,
    java_index: &JavaIndex,
    local_symbols_by_file: &HashMap<String, HashSet<String>>,
) -> Vec<SymbolEdge> {
    // Memoized per specifier: a tree with N calls into one service resolves that service's specifier N
    // times otherwise, and `resolve_java_import`'s trim walk is not free.
    let mut resolved: HashMap<&str, Vec<String>> = HashMap::new();
    let mut out: Vec<SymbolEdge> = Vec::new();
    for edge in symbol_graph {
        let Some((specifier, name)) = edge.to.split_once('#') else {
            continue;
        };
        // A path-shaped file part is already in the real-file space — nothing to bridge, and no reason to
        // pay the index lookup. Dotted specifiers carry no separator and no source extension; the
        // extension test is what excludes a `.java` file sitting at the tree root, whose rel has no
        // separator either and would otherwise reach the index as if it were a package-qualified name.
        if specifier.contains('/')
            || specifier.contains('\\')
            || specifier.ends_with(".java")
            || !specifier.contains('.')
        {
            continue;
        }
        let files = resolved
            .entry(specifier)
            .or_insert_with(|| resolve_java_import(specifier, java_index));
        for file in files.iter() {
            if local_symbols_by_file
                .get(file)
                .is_some_and(|names| names.contains(name))
            {
                out.push(SymbolEdge {
                    from: edge.to.clone(),
                    to: format!("{file}#{name}"),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    out.dedup_by(|a, b| a.from == b.from && a.to == b.to);
    out
}
