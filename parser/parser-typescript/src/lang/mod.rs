//! The swc -> Common-IR LANGUAGE projection: TypeScript/JavaScript call-site and module-resolution
//! machinery not tied to any framework's vocabulary. Symbol/import extraction (`parse_symbols`/
//! `parse_imports`) stays in the crate root `lib.rs` since every module here depends on it; only the
//! two separable pieces — call-graph construction and dependency-path resolution — live here. See
//! `adapters`'s module doc for the sibling half of this crate's 2-layer layout.

pub mod calls;
pub mod resolve;
pub mod write_site;
