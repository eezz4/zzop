//! Score types — per-metric violation and summary structs, plus the aggregate `Scores` collection.
//! Rust field names stay snake_case per Rust/crate convention (see e.g. node.rs FileNode); every struct
//! here carries `#[serde(rename_all = "camelCase")]` so the WIRE (JSON) shape matches every other
//! napi-boundary output type instead — see `crates/facade/src/lib.rs`'s `AnalyzeOutputView` doc.

mod reports;
mod violations;

pub use reports::*;
pub use violations::*;
