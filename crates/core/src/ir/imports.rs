//! Import / re-export declaration facts — split out of `ir.rs` purely to keep that file under the
//! line-count ratchet. Nothing about the split is semantic: these three items are re-exported from
//! `ir` unchanged, so every `zzop_core::ImportBinding`/`ImportMap`/`ReExport` path is untouched, and
//! `ir.rs` remains the single owner of the SYMBOL contract every parser doc points at (its "Body span
//! contract" section) — that pointer must not be made to move by a mechanical file split.

use serde::{Deserialize, Serialize};
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
