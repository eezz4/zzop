//! ruff AST -> Common-IR LANGUAGE projection: symbols, imports, identifier references, plus a pure
//! import-specifier candidate resolver (`resolve`), plus the `RawCall` call-site projection (`calls`)
//! the engine's whole-repo `SymbolGraph` is built from. Mirrors `zzop_parser_typescript`'s split of
//! concerns.

pub mod calls;
pub mod imports;
pub mod resolve;
pub mod symbols;
pub mod used_names;
