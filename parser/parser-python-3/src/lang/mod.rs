//! ruff AST -> Common-IR LANGUAGE projection: symbols, imports, identifier references, plus a pure
//! import-specifier candidate resolver (`resolve`), plus the `RawCall` call-site projection (`calls`)
//! the engine's whole-repo `SymbolGraph` is built from. Mirrors `zzop_parser_typescript`'s split of
//! concerns.
//!
//! `calls` and `call_sites` are neighbours with different jobs, and the names are close enough to
//! warrant saying which is which: `calls` projects EVERY call for graph building (edges between
//! symbols), while `call_sites` projects only the few API FAMILIES `zzop_core::CallSite` names
//! (console writes, env reads) for the DSL to judge. Widening `call_sites` toward "every call" is the
//! collapse its channel doc forbids.

pub mod call_sites;
pub mod calls;
pub mod imports;
pub mod loop_spans;
pub mod resolve;
pub mod string_literals;
pub mod symbols;
pub mod used_names;
