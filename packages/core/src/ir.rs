//! Common IR — the language-neutral intermediate representation (the lower layer of the 2-layer IR).
//! Parsers project a Normalized AST into this Common IR, then drop the AST (memory safety).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::io::IoFacts;

/// madge-compatible dep graph: `{ sourcePath: [importedPath, ...] }`.
pub type DepGraph = HashMap<String, Vec<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceSymbolKind {
    Function,
    Class,
    Const,
    Type,
    Interface,
}

/// Classifies a store-write call as non-idempotent for `zzop_rules_http::http_scan`'s
/// `non-idempotent-write` rule: a retry of any of these effects is not a no-op. `as_str` gives the
/// wire/label form used both in `Finding::data.kind` and (via serde) in the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NonIdempotentKind {
    /// `create`/`createMany`/`insert` — a retry inserts a duplicate row.
    Create,
    /// An `update`/`updateMany`/`upsert` whose data carries an atomic accumulation op
    /// (`increment`/`decrement`/`push`/`multiply`) — a retry applies the delta again.
    AtomicAccumulate,
    /// A counter-store bump (`incr`/`incrby`/`decr`/`decrby`) — a retry bumps it again.
    Counter,
}

impl NonIdempotentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::AtomicAccumulate => "atomic-accumulate",
            Self::Counter => "counter",
        }
    }
}

/// One detected store-write (ORM/atomic/counter or raw-SQL) call site within a symbol's body span —
/// computed once at parse time (TS only; see `zzop_parser_typescript`'s write-site detection module)
/// and carried on `SourceSymbol::write_sites`. `scan_unsafe_read_endpoint` treats the presence of a
/// site as a write regardless of `kind`; `scan_non_idempotent_write` additionally requires `kind` to
/// be set and allowed for the endpoint's HTTP method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteSite {
    pub file: String,
    /// 1-based line of the write call/statement.
    pub line: u32,
    /// Short human-readable label (`"prisma.user.update"`, or the first few tokens of a raw-SQL statement).
    pub sink: String,
    /// Set only when the write also qualifies as non-idempotent (create/atomic-accumulate/counter);
    /// `None` for a plain idempotent update or a raw-SQL write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<NonIdempotentKind>,
}

/// A top-level symbol within a file.
///
/// ## Casing: uniform camelCase OUTPUT, snake_case still accepted on INPUT
/// Dual-purpose type: an OUTPUT shape (`CommonIr`'s `symbols`, via `MinimalIr`) AND the exact type
/// `docs/NORMALIZED_AST.md`'s frozen v1 `FileProjection.symbols` external-parser input contract
/// deserializes (see `normalized.rs`, which reuses this struct verbatim). `#[serde(rename_all =
/// "camelCase")]` makes the OUTPUT uniform with every other output-facing type in this crate; the
/// per-field `#[serde(alias = ...)]` attributes keep the frozen v1 snake_case INPUT names
/// (`is_default`/`body_start`/`body_end`) deserializing alongside the new camelCase ones — additive
/// on input, unifying on output, not a breaking rename.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSymbol {
    /// "features/x/useFoo.ts#useFoo" — file + name combination id.
    pub id: String,
    /// Normalized relative path.
    pub file: String,
    pub name: String,
    pub kind: SourceSymbolKind,
    /// Declaration start line (1-based).
    pub line: u32,
    pub exported: bool,
    /// `export default function` — also matchable via the `file#default` key.
    #[serde(
        default,
        alias = "is_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub is_default: bool,
    /// Body line range for functions/classes (None for type/interface).
    #[serde(default, alias = "body_start", skip_serializing_if = "Option::is_none")]
    pub body_start: Option<u32>,
    #[serde(default, alias = "body_end", skip_serializing_if = "Option::is_none")]
    pub body_end: Option<u32>,
    /// Pre-computed store-write sites within this symbol's body span, in source order — computed once
    /// at parse time (TS only; empty for non-TS/degraded/type symbols). Feeds
    /// `zzop_rules_http::http_scan`'s `unsafe-read-endpoint`/`non-idempotent-write` call-graph scanners.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_sites: Vec<WriteSite>,
}

/// One resolved Prisma query call site: `<clientAccessor>().<modelAccessor>.<method>(...)`, using the
/// same `getPrisma()`-style accessor vocabulary as `zzop_rules_schema::usage::scan_store_map`.
///
/// A per-file parser fact, like `SourceSymbol` above: `zzop_parser_typescript::extract_query_call_sites`
/// produces one file's sites during the fused per-file pass, `zzop_cache::FileIrSlice` round-trips them
/// through the cache, and `zzop_engine::analyze::assemble` collects every file's sites into one
/// tree-wide `Vec` for `zzop_rules_schema::join`'s three schema x usage JOIN rules
/// (`soft-delete-bypass`/`orderby-unindexed`/`enum-string-drift`) to scan — mirroring how
/// `trpc_router_fragments` travels from parser to engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryCallSite {
    /// PascalCase model name, derived by capitalizing the camelCase client accessor (`item` -> `Item`).
    pub model: String,
    /// One of `findMany` / `findFirst` / `findUnique` / `count`.
    pub method: String,
    pub file: String,
    /// 1-based line of the method-call token itself.
    pub line: u32,
    /// The balanced-paren argument span, `(...)` inclusive — raw source text, comments/strings not stripped.
    pub call_text: String,
}

/// An import-declaration binding. Keyed by localName.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportBinding {
    /// Verbatim specifier from the import "..." statement ("@/features/x", "./foo").
    pub specifier: String,
    /// Original exported name: default import = "default", namespace = "*".
    pub original: String,
    /// A CommonJS `require()` nested in a function body — a lazy import (does not affect module load order).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deferred: bool,
    /// Type-only (`import type ...` or `import { type X }`). Erased by TS at compile time.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub type_only: bool,
}

pub type ImportMap = std::collections::BTreeMap<String, ImportBinding>;

/// A re-export. `export { A as B } from "./y"` / `export * from "./y"`. A non-type-only re-export is a
/// real dep-graph edge (`zzop_parser_typescript::lang::resolve::build_dep`/`build_dep_with_workspace`
/// resolve+merge it into the same `resolved` vector an `ImportBinding` would); a type-only one
/// (`export type { X } from "./y"` / per-specifier `export { type X } from "./y"`) is erased by TS at
/// compile time and contributes no edge at all — mirrors `ImportBinding::type_only`'s
/// erased-at-compile-time semantics, but for re-exports the effect is "no edge" rather than "edge that's
/// excluded from circular only" since a re-export's only purpose in the dep graph is the edge itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReExport {
    /// Specifier from `export ... from "..."`.
    pub specifier: String,
    /// Original name in the source. star = "*".
    pub original: String,
    /// Name exposed in the current file. `export { A as B }` = B, star = "*".
    pub local_alias: String,
    /// Type-only (`export type { X } from "..."` or a per-specifier `export { type X } from "..."`).
    /// Erased by TS at compile time — never a dep-graph edge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub type_only: bool,
}

/// A Hono-style endpoint extracted from a backend route file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    pub handler: String,
}

/// The minimal IR a parser must produce.
/// `dep` = internal import edges, `symbols` = exported declarations, `loc` = rel -> non-blank/non-comment line count.
/// `io` (optional) = the parser projects its framework boundaries to normalized contract keys (cross-layer join input).
/// `#[serde(rename_all = "camelCase")]` is a no-op today (every field is one word) — kept for
/// consistency with every other output-facing type in this crate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimalIr {
    /// `HashMap` iteration order is hasher-randomized per process — `serialize_with` sorts keys so
    /// `ir.dep` serializes byte-deterministically across runs (see `crate::serde_util::sorted_map`'s
    /// doc). Deserialize is untouched: a JSON object's key order never affects which entries land in the
    /// resulting map.
    #[serde(serialize_with = "crate::serde_util::sorted_map")]
    pub dep: DepGraph,
    pub symbols: Vec<SourceSymbol>,
    /// See `dep`'s doc — same determinism fix.
    #[serde(serialize_with = "crate::serde_util::sorted_map")]
    pub loc: HashMap<String, u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io: Option<IoFacts>,
}

/// One source tree's Common IR — the unit the engine and rules consume.
/// Tree-by-tree streaming: parse -> project to this IR -> drop the AST.
/// `#[serde(rename_all = "camelCase")]` is a no-op today (`source`/`parser` are already one word) —
/// kept for consistency with every other output-facing type in this crate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonIr {
    /// Source (repo/service) id — used to tag trees during the cross-layer join.
    pub source: String,
    /// Id of the parser/adapter that produced this tree (e.g. "typescript", "java", "jsp").
    pub parser: String,
    #[serde(flatten)]
    pub ir: MinimalIr,
}
