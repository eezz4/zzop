//! The FUSE step: the point where several parser frontends stop being separate and become one
//! tree-wide `CommonIr`, plus the census computed over it.
//!
//! Split from `super` when that file crossed the 300-line cap, and this is the step that cut cleanly
//! because it is the only one whose inputs are all FINISHED. Every other statement in `assemble` is
//! mid-pipeline — it takes a substrate and produces another substrate — so lifting one costs more
//! parameter plumbing than it removes (measured: two other carves were tried and both made the file
//! LONGER). Here the dep graph, symbols, loc map and io facts are done being built, which is exactly
//! what "fuse" means, and the census is the first thing that reads the fused result.

use zzop_core::{CommonIr, DepGraph, MinimalIr, SourceSymbol};

use crate::{CoverageCensus, EngineConfig};

/// Everything the fuse consumes. Owned where the IR takes ownership, borrowed where the census only
/// reads — the split is the same one the two calls below make, so a field that is borrowed here is one
/// the caller still needs afterwards.
pub(super) struct FuseInputs<'a> {
    pub(super) config: &'a EngineConfig,
    pub(super) dep: DepGraph,
    pub(super) all_symbols: Vec<SourceSymbol>,
    pub(super) loc_by_path: std::collections::HashMap<String, u32>,
    pub(super) io: Option<zzop_core::IoFacts>,
    pub(super) file_count: usize,
    pub(super) parser_dispatched: usize,
    pub(super) degraded: usize,
    pub(super) ts_import_pairs: &'a [(String, zzop_core::ImportMap)],
    pub(super) ts_re_export_pairs: &'a [(String, Vec<zzop_core::ReExport>)],
    pub(super) ts_dynamic_import_pairs: &'a [(String, Vec<String>)],
}

/// The fused IR and the census over it.
pub(super) fn ir_and_census(i: FuseInputs<'_>) -> (CommonIr, CoverageCensus) {
    let ir = CommonIr {
        source: i.config.source_id.clone(),
        // Multiple parser frontends (TypeScript + Prisma, v1 scope) fuse into one tree-wide IR here, so
        // no single `parser` id is accurate the way it is for a single-frontend `build_common_ir` call —
        // this is a zzop-only tag naming the fused engine itself rather than one frontend.
        parser: "engine".to_string(),
        ir: MinimalIr {
            dep: i.dep,
            symbols: i.all_symbols,
            loc: i.loc_by_path,
            io: i.io,
        },
    };

    let mut coverage = CoverageCensus::compute(i.file_count, i.parser_dispatched, &ir, i.degraded);
    // F4 declared-import denominator — set here, not in `compute` (see `super::declared`'s module doc).
    coverage.declared_imports_by_ext = super::declared::by_ext(
        i.ts_import_pairs,
        i.ts_re_export_pairs,
        i.ts_dynamic_import_pairs,
    );
    (ir, coverage)
}
