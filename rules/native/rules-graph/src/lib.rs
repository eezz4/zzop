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

use zzop_core::rule_channels::reads::NO_IO;
use zzop_core::{
    declare_native_rule_channels, register_native_analysis_stub, NativeRuleChannels, RuleIoChannel,
    RuleRegistry,
};

/// Every native analysis id this crate owns, each on one row with the cross-layer io channels its rule
/// body reads (`zzop_core::rule_channels`' mechanism doc holds the contract). ONE table, two readers —
/// [`register_native_analyses`] and [`native_rule_channels`] — so an id cannot be registered without a
/// channel statement, and the statement cannot drift from the id it describes.
///
/// Every row here reads [`NO_IO`], and that is a property of the crate rather than a coincidence: these
/// rules judge the dependency/dead-code graph and the symbol call graph, which are extracted per file and
/// never enter the cross-layer io join. `rule_contracts::rule_channels` checks the claim the only way it
/// can be checked — this crate's own source names no io kind and constructs no `IoProvide`/`IoConsume`.
const NATIVE_ANALYSES: &[(&str, &[RuleIoChannel])] = &[
    ("circular", NO_IO),
    ("unreachable", NO_IO),
    ("dead-candidates", NO_IO),
    ("unimported-export", NO_IO),
    ("cache-lane-file-read", NO_IO),
];

/// Registers every native analysis id whose implementation lives in this crate (see `rules/README.md`'s
/// "Adding a rule" section); `zzop_engine::register_all_native` composes this with the other crates' own.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    for row in native_rule_channels() {
        register_native_analysis_stub(registry, &row.rule_id);
    }
}

/// This crate's half of the rule→io-channel declaration, composed with the other crates' own by
/// `zzop_engine::native_rule_channels` — the same aggregator shape as [`register_native_analyses`].
pub fn native_rule_channels() -> Vec<NativeRuleChannels> {
    declare_native_rule_channels(NATIVE_ANALYSES)
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
