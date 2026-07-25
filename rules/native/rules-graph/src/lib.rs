//! zzop-rules-graph — native rules that operate over a repo's dependency/dead-code graph, git-free.
//!
//! ## Module map
//! - [`circular`]: Finding-shaping for `"circular"` (the algorithm itself lives in `zzop_core::graph`).
//! - [`unreachable`]: closed "dead island" file detection.
//! - [`dead_candidates`]: fanIn == 0 candidate dead files.
//! - [`dead_exports`]: symbol-level dead-export detection.
//!
//! HTTP/route rules live in `zzop-rules-http`; multi-tree cross-layer join rules live in
//! `zzop-rules-cross-layer` — both were split out of this crate (see `docs/ARCHITECTURE.md`).
//!
//! Every rule body here depends on `zzop-core` only.

pub mod circular;
pub mod dead_candidates;
pub mod dead_exports;
pub mod unreachable;

use zzop_core::{register_native_analysis_stub, RuleRegistry};

/// Registers every native analysis id whose implementation lives in this crate (see `rules/README.md`'s
/// "Adding a rule" section); `zzop_engine::register_all_native` composes this with the other crates' own.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    for id in ["circular", "unreachable", "dead-candidates", "dead-exports"] {
        register_native_analysis_stub(registry, id);
    }
}

pub use circular::circular_findings;
pub use dead_candidates::{dead_candidate_findings, find_dead_candidates, DEAD_MAX_CHANGES};
pub use dead_exports::{
    dead_export_findings, find_dead_exports, DeadExport, DeadExportCandidate, DeadExportInputFile,
    DeadExportReason,
};
pub use unreachable::{find_unreachable, unreachable_findings, UnreachableFile};
