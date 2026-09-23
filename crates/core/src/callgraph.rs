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

pub use bfs::{bfs_reachable_in, Adjacency};
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

/// Everything ONE TypeScript file contributes to the whole-tree call-graph pass, gathered while that
/// file's AST is already in hand.
///
/// The pass used to read and re-parse every dispatched source itself, which cost a SECOND full swc
/// parse per file: `parse_with_cm`'s memo is one entry and thread-local, so it collapses the per-file
/// lane's consecutive extractors and can never be warm for a pass that runs later on another thread.
/// Measured on this repository: two parses per `.ts` file, and the pass was 68% of a warm run (review
/// ledger V103/V108). Moving the extraction to where the parse already happened makes it one, and puts
/// the result behind the per-file cache — which is where the 68% actually lived.
///
/// The three guard fields are NestJS-specific and TypeScript-only, the same unevenness `function_spans`
/// (TypeScript) and `test_spans` (Rust) already carry: a fact belongs to the languages that can produce
/// it, and an empty vec here means this file had none, never that the question was not asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallGraphFacts {
    /// This file's call sites, attributed to their enclosing symbol. Cross-file resolution into
    /// `SymbolEdge`s is still [`resolve_calls_for_file`]'s job — this is the per-file half only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_calls: Vec<RawCall>,
    /// Lines where a controller-level guard decorator applies (`extract_controller_guarded_lines`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controller_guarded_lines: Vec<u32>,
    /// Nest `forRoutes` (method, path) patterns. Consumed with `.any(..)`, so order cannot reach a
    /// verdict.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nest_forroutes: Vec<(String, String)>,
    /// This file's `setGlobalPrefix` marker, if it declares one. The tree-wide winner is the LOWEST
    /// path, which is the consumer's decision, not this file's — so every candidate is carried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_prefix: Option<String>,
}

impl CallGraphFacts {
    /// True when this file contributed nothing — the common case, and what lets a cached slice skip
    /// the whole object rather than serialize four empty containers per file.
    pub fn is_empty(&self) -> bool {
        self.raw_calls.is_empty()
            && self.controller_guarded_lines.is_empty()
            && self.nest_forroutes.is_empty()
            && self.global_prefix.is_none()
    }
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
