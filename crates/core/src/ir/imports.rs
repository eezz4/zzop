//! Import / re-export declaration facts — split out of `ir.rs` purely to keep that file under the
//! line-count ratchet. Nothing about the split is semantic: these three items are re-exported from
//! `ir` unchanged, so every `zzop_core::ImportBinding`/`ImportMap`/`ReExport` path is untouched, and
//! `ir.rs` remains the single owner of the SYMBOL contract every parser doc points at (its "Body span
//! contract" section) — that pointer must not be made to move by a mechanical file split.

use serde::{Deserialize, Serialize};
/// An import-declaration binding. The [`ImportMap`] key is, for most front ends, the name the
/// importing FILE BINDS — the `<localName>` spelling `docs/NORMALIZED_AST.md` uses for this map — but
/// that is not universal, and this file said it was, unconditionally, until 2026-08-14.
///
/// The key is load-bearing for anyone outside this crate: it is what an overlay adapter must match to
/// DISPLACE a native binding (`docs/NORMALIZED_AST.md`'s `overrides` section) and what the native-first
/// merge compares. A consumer that assumes "local name" everywhere does not get an error when it is
/// wrong — its binding lands as a SIBLING of the one it meant to replace and both edges survive, which
/// is exactly the failure that section already measured on Python.
///
/// Measured from the front ends themselves (`ImportBinding` construction sites under `parser/*/src`),
/// and held against them in both directions by
/// `crates/core/tests/envelope_schema_parity/import_key_table.rs`:
///
/// | Front end | Key of a name-binding import | Keys that are NOT a local name |
/// |---|---|---|
/// | TypeScript | the bound local name: an `as` alias when written, else the imported/default/namespace ident; `const X = require("y")` binds `X`, a destructured one binds each local | `__require{N}__` (a bare or inline `require("y")` that binds nothing) |
/// | Python | the name Python itself binds: `import a.b.c` binds `a`, an `as` alias binds the alias, `from x import n` binds `n` | `__star_import_{N}__` |
/// | Java | the dotted name's rightmost segment (`import a.b.C` binds `C`; `import static a.b.C.m` binds `m`) | `__glob_import_{N}__` |
/// | C# | **the FULL specifier** for a plain `using A.B;` — see below; the `using X = A.B;` alias form keys the alias | `__static_import_{N}__` |
/// | Rust | the `use` leaf name or its rename; a bodiless `mod x;` binds `x` | `__glob_import_{N}__` |
/// | Go | the alias when written, else the import path's last slash-separated segment | `__{label}_import_{N}__`, label being `dot` or `blank` |
/// | Prisma | — | — |
/// | SQL | — | — |
///
/// **C# is the deviation, and it is deliberate.** `using A.Models;` and `using B.Models;` are both
/// legal in one file, so a last-segment key would collide in this `BTreeMap` and silently drop one
/// witnessed namespace from the dep graph and the census; `zzop_parser_csharp::lang::imports` keys the
/// full specifier instead and owns the derivation. The consequence for an adapter is concrete: a C#
/// import overlay must key by SPECIFIER, not by the simple name.
///
/// **The synthetic keys are a shared convention, not a per-language quirk**: an import that binds no
/// single local name (a star/glob/dot/blank/side-effect import) still gets a collision-free key of the
/// shape shown, so the EDGE enters the map instead of being dropped. Nothing may parse them for
/// meaning — the specifier is where the information is.
///
/// **The dashed rows are front ends that construct no `ImportBinding` at all** — PSL has no imports,
/// and the SQL front end projects no module graph. A consumer asking those trees for `imports` is
/// asking a question the language does not have, which is a different answer from "none found" (the
/// same distinction the dashes in `SourceSymbolKind`'s collapse table draw — `ir/kinds.rs`).
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
