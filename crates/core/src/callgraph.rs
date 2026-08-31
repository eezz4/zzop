//! Call-graph substrate — `RawCall` (parser-projected intra-file call sites) -> `SymbolEdge`/`SymbolGraph`
//! (cross-file-resolved caller-symbol -> callee-symbol edges) -> BFS reachability over that graph.
//! Downstream direction only — the only direction the call-graph rules in `rules/native/rules-graph` need.
//!
//! Backs the `rules/native/rules-graph` HTTP-handler-reachability rules (`scanUnsafeReadEndpoint` /
//! `scanNonIdempotentWrite`, both BFS-over-`symbolEdges` from an HTTP handler symbol to a store-write call).
//!
//! The two halves split along the substrate's own seam and are re-exported flat, so this module's public
//! surface is unchanged by the split: [`resolve`] turns raw calls into edges (and reports what it could
//! not place), [`bfs`] walks the finished edge list and knows nothing about how it was built.

use serde::{Deserialize, Serialize};

mod bfs;
mod resolve;

pub use bfs::bfs_reachable;
pub use resolve::{
    build_symbol_graph, build_symbol_graph_with_unresolved, resolve_calls_for_file,
    resolve_calls_for_file_with_unresolved,
};

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

/// The whole-repo symbol call graph: a flat edge list. [`bfs`]'s helpers build their adjacency index
/// from this on demand.
pub type SymbolGraph = Vec<SymbolEdge>;

#[cfg(test)]
mod tests;
