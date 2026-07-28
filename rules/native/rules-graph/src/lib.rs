//! zzop-rules-graph — native rules that operate over a repo's dependency/dead-code graph, git-free.
//!
//! ## Module map
//! - [`circular`]: Finding-shaping for `"circular"` (the algorithm itself lives in `zzop_core::graph`).
//! - [`unreachable`]: closed "dead island" file detection.
//! - [`dead_candidates`]: fanIn == 0 candidate dead files.
//! - [`dead_exports`]: symbol-level dead-export detection.
//! - [`cache_lane_file_read`]: a declared cached lane reaching a declared filesystem read — the same
//!   BFS-plus-vocabulary shape as `rules-http`'s `mutating-route-no-auth`, with the verdict's polarity
//!   flipped (reaching the vocabulary IS the finding).
//!
//! HTTP/route rules live in `zzop-rules-http`; multi-tree cross-layer join rules live in
//! `zzop-rules-cross-layer` — both were split out of this crate (see `docs/ARCHITECTURE.md`).
//!
//! Every rule body here depends on `zzop-core` only.

pub mod cache_lane_file_read;
pub mod circular;
pub mod dead_candidates;
pub mod dead_exports;
pub mod unreachable;

use zzop_core::{register_native_analysis_stub, RuleRegistry};

/// Registers every native analysis id whose implementation lives in this crate (see `rules/README.md`'s
/// "Adding a rule" section); `zzop_engine::register_all_native` composes this with the other crates' own.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    for id in [
        "circular",
        "unreachable",
        "dead-candidates",
        "unimported-export",
        "cache-lane-file-read",
    ] {
        register_native_analysis_stub(registry, id);
    }
}

pub use cache_lane_file_read::{
    scan_cache_lane_file_read, CacheLaneCallSites, ScanCacheLaneFileReadInput,
    DEFAULT_FILE_READ_CALLEES,
};
pub use circular::circular_findings;
pub use dead_candidates::{dead_candidate_findings, find_dead_candidates, DEAD_MAX_CHANGES};
pub use dead_exports::{
    dead_export_findings, find_dead_exports, DeadExport, DeadExportCandidate, DeadExportInputFile,
    DeadExportReason,
};
pub use unreachable::{find_unreachable, unreachable_findings, UnreachableFile};
