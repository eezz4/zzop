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

/// A re-export. `export { A as B } from "./y"` / `export * from "./y"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReExport {
    /// Specifier from `export ... from "..."`.
    pub specifier: String,
    /// Original name in the source. star = "*".
    pub original: String,
    /// Name exposed in the current file. `export { A as B }` = B, star = "*".
    pub local_alias: String,
}

/// A Hono-style endpoint extracted from a backend route file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    pub handler: String,
    /// `// drift-ok: <reason>` marker — an intentionally dead route (whitelist for be-route-not-called).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drift_ok: bool,
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
