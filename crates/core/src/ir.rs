//! Common IR — the language-neutral intermediate representation (the lower layer of the 2-layer IR).
//! Parsers project a Normalized AST into this Common IR, then drop the AST (memory safety).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::io::IoFacts;

/// madge-compatible dep graph: `{ sourcePath: [importedPath, ...] }`.
pub type DepGraph = HashMap<String, Vec<String>>;

/// The ONE sentence every surface that publishes a `DepGraph`-derived number must show, and its only
/// owner. Reference this const (or link to it from a doc); never restate the rule — it was already
/// implied in five places under five different spellings, which is how the misread below survived.
///
/// # Why it has to be said at all
/// `dep` is built by RESOLUTION, not transcription: the assemble phase keeps an edge only when the
/// import specifier resolved to a file the walk actually visited, so an import of a published package
/// and a specifier no resolver could map are BOTH dropped (`AnalyzeOutput::package_imports` is the one
/// place package imports survive, and only as a per-package importing-file set). Every number derived
/// from this graph therefore counts in-tree edges only — the census's `resolvedImportEdges`, a node's
/// `fanIn`/`fanOut`/`degree`, a critical file's `blastRadius`, `queryFile`'s `dependencies`. The
/// founding misread: a 91-file Python tree reported 3 import edges and was read as "this repo barely
/// imports anything", when what it actually said was "3 of its imports resolved to files in this tree".
pub const DEP_GRAPH_RESOLVED_ONLY: &str =
    "zzop's dependency graph holds ONLY resolved in-tree edges: an import of a published package \
     (npm/Maven/pip/...) and a specifier the resolver could not map to a walked file are both dropped, \
     so every number derived from this graph counts in-tree edges only and a low one can mean \
     unresolved imports rather than few imports.";

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
/// ## Body span contract (`body_start`/`body_end`) — ONE convention, owned here
/// Every producer (the eight `parser/*` crates AND any external adapter feeding
/// `docs/NORMALIZED_AST.md`'s `FileProjection`) owes exactly this, and no parser module doc restates
/// it — they link here, because until 2026-08-10 these two fields carried a two-line comment and six
/// parsers had grown THREE readings of it (TS/Java/C#: the body BLOCK's `{` line; Go/Rust/Python: the
/// FIRST STATEMENT's line; TS/Java/C# containers: the declaration's own line).
///
/// **`body_start` is the line the DECLARATION begins on — its leading decorators, annotations and
/// attributes INCLUDED. `body_end` is the last line of what that declaration encloses.** Both are
/// 1-based and inclusive; the pair is the region `dsl::method_scan` scans and `drop_outer_spans`
/// nests, and `capability_matrix`'s `decl_in_span` column proves each environment still obeys it.
///
/// ### Why the declaration line and not the body's first line
/// Because that is what the RULES mean by "in this method", and the three readings agreed on it only
/// by accident. `rules/dsl/typescript/typescript.json`'s `async-handler-no-try` triggers on
/// `\bon[A-Z]\w*\s*[:=]\s*\{?\s*async\b` — a pattern that can only match a DECLARATION line — and it
/// worked solely because swc's block span starts there whenever the author put the `{` on that line.
/// Under the block-line reading the same concept was unwritable for Python/Go/Rust (`async def
/// handler():` is never inside any span) and silently unwritable for TS/Java/C# too the moment a
/// signature wrapped. Every portable method-scan concept is anchored on the declaration — `async`,
/// `@Transactional`, `[HttpGet]`, `#[get(...)]`, a parameter name — so a span excluding it excludes the
/// question. Three further properties decided it. TOTAL: every language has a declaration start line,
/// whereas "first statement" does not exist for an empty body, a comment-only body, or an expression
/// body (`int P => Compute();`) — each of which used to collapse the whole span to `None`,
/// a silent and total loss of scannability. ALREADY PRECEDENT: TS classes, Java types and C# types
/// used it already, so pinning it unifies with three producers rather than inventing a fourth reading.
/// CONTAINMENT-PRESERVING: a container's declaration line precedes every member's and a member's
/// precedes its own body, so `drop_outer_spans`'s innermost-wins nesting and `parse_calls`'s
/// smallest-range attribution keep working unchanged. The cost is stated rather than hidden: the
/// parameter list and the annotation ARGUMENTS are in the span now, so a rule's `patterns` AND its
/// `absent` guards can both match there — the point in both directions, since a guard written as
/// `@Transactional(readOnly = true)` should veto.
///
/// ### `None` means "there is no region", never "this producer could not compute one"
/// `None`/`None` is a positive claim that the declaration encloses nothing scannable, and must stay
/// `None` wherever that holds — a Rust `struct`/`enum`/`union`/`trait`, a Go `type X struct`, a TS
/// `type`/`interface`, any field or const, a Prisma model, an abstract method declared with `;`. Those
/// carry a FIELD or SIGNATURE list, not a body: a span over one would make `drop_outer_spans` treat it
/// as scannable and claim a per-member containment the language does not have.
///
/// ### Leaf completeness — the producer obligation that comes with the span
/// `drop_outer_spans` discards a container span that strictly contains another projected span, so a
/// container's regions are reachable ONLY through the leaves its producer emits. A producer that gives
/// a container a span therefore owes a leaf for **every** region of it a rule could need to scan — a
/// Java `static { … }`/instance initializer, a Python class-body statement run, a C# expression-bodied
/// property — or that region goes unreachable the instant any sibling projects a leaf. Making the
/// discard conditional instead was measured and rejected; `drop_outer_spans`'s own doc carries why.
///
/// ## Casing: uniform camelCase OUTPUT, snake_case still accepted on INPUT
/// Dual-purpose type: an OUTPUT shape (`CommonIr`'s `symbols`, via `MinimalIr`) AND the exact type
/// `docs/NORMALIZED_AST.md`'s `FileProjection.symbols` external-parser input contract
/// deserializes (see `normalized.rs`, which reuses this struct verbatim). `#[serde(rename_all =
/// "camelCase")]` makes the OUTPUT uniform with every other output-facing type in this crate; the
/// per-field `#[serde(alias = ...)]` attributes keep the long-standing snake_case INPUT names
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
    /// First line of the scannable region: the DECLARATION's own start line, decorators/annotations/
    /// attributes included — see this struct's "Body span contract" doc section, which is the sole
    /// owner of the rule and of what `None` claims. Both fields are `Some` together or `None`
    /// together; a half-populated pair is a producer bug (`dsl::method_scan` skips such a symbol).
    #[serde(default, alias = "body_start", skip_serializing_if = "Option::is_none")]
    pub body_start: Option<u32>,
    /// Last line of the scannable region, 1-based and INCLUSIVE — see the "Body span contract" above.
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
/// `procedure_router_fragments` travels from parser to engine.
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
/// `dep` = internal import edges, `symbols` = exported declarations, `loc` = rel -> RAW physical line
/// count (`text.split('\n').count()`, matching `zzop_parser_typescript::count_loc`) — blank lines,
/// comment-only lines, and lines inside a block comment/multi-line string all count. It is NOT a
/// "meaningful code lines" count the name might suggest; nothing here excludes blank/comment lines.
/// EVERY shipped frontend applies that one rule — typescript, python-3, java-21, csharp, rust, go and
/// (since 2026-07-31) prisma.
///
/// `parser-prisma` was the lone deviation until then: its `count_schema_loc` dropped blank and
/// `//`-comment lines, and this doc recorded the split rather than closing it ("read a Prisma tree's
/// `loc` as meaningful-lines"). That was not enough, and the reason is the wire, not the arithmetic:
/// `loc` is ONE field name, a multi-tree reply can carry a Prisma tree beside a TypeScript one, and
/// nothing in the reply marks which definition a given row used. A reader summing or comparing `loc`
/// across trees was adding two different measurements. Prisma was aligned to the majority rule (user
/// ruling) — a schema's `loc` rose by its blank and comment lines — because one definition per field
/// name is the property that makes the number comparable at all.
///
/// So `loc` is PHYSICAL LINES, everywhere, with no per-parser exception. A frontend that wants a
/// meaningful-lines measure must publish it under its OWN name, not this one.
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
