//! ruff AST -> Common-IR LANGUAGE projection: symbols, imports, identifier references, plus a pure
//! import-specifier candidate resolver (`resolve`). Mirrors `zzop_parser_typescript`'s split of
//! concerns, minus a `calls` module (call-graph construction stays out of v1 scope — only the
//! TypeScript frontend builds one).

pub mod imports;
pub mod resolve;
pub mod symbols;
pub mod used_names;
