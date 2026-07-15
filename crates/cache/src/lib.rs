//! zzop-cache — the file-level analysis cache.
//!
//! Two separate on-disk entries per file: the Common IR slice (keyed by content hash + parser
//! fingerprint) and per-file rule findings (same key + ruleset fingerprint), so a rule-pack-only
//! change invalidates findings but keeps the parsed IR reusable. Whole-graph passes are never cached —
//! they're a cheap linear combination of the per-file IRs. Every entry is file-independent, so this is
//! safe to drive from a `rayon` file-parallel walk.

mod hash;
mod ir_slice;
mod key;
mod store;

pub use ir_slice::FileIrSlice;
pub use key::CacheKey;
pub use store::AnalysisCache;
