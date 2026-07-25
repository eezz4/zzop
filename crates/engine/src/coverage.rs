//! Structural coverage census — a vocab-free, per-tree count of which analysis channels this tree
//! actually filled. It is a pure post-aggregate of the already-assembled `AnalyzeOutput` data (io / dep /
//! symbols / degraded / file_count): "key present means it ran", so a consumer can tell "analyzed and
//! found 0" apart from "this channel was dark". `join_contribution_zero` is the active-blindness FACT
//! (not a heuristic): a tree that contributed NO io to the cross-layer join. See
//! decision doc coverage-disclosure.md (Stage 1).
//!
//! **Not test coverage**: despite the filename, nothing here measures which lines a test suite exercises.
//! "Coverage" means analysis-CHANNEL fill — did the io/dep/symbol extractors find anything for this tree
//! at all — not `pytest --cov`/`nyc`-style executed-line percentages.

use zzop_core::CommonIr;

/// Vocab-free per-tree channel-fill census. All counts are kind-agnostic (every io kind, not just http).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoverageCensus {
    /// Files the walk visited (== `AnalyzeOutput::file_count`). The walk applies no extension filter, so
    /// this counts EVERY file under the root that survived gitignore/skip-dir pruning — docs, data,
    /// lockfiles and binary assets included. Use [`source_files`](Self::source_files) to size the code.
    pub files: usize,
    /// The subset of `files` a parser actually claims: a native frontend dispatched on it
    /// (`dispatch::dispatch` returned a language) or an APPLIED adapter overlay covers it. This is the
    /// honest "how much code is in this tree" number.
    ///
    /// Added because `files` alone was being misread as the repo's code size: a field run reported
    /// `fileCount: 4790` on a tree with roughly 3,178 code files, and an agent sizing the repo from that
    /// number over-estimates by half. `files` was NOT redefined — it is documented as "files walked" and
    /// is honest at that job (renaming or narrowing it would silently change a published output field, and
    /// several internal gates key off it, e.g. `join_contribution_zero`'s `files > 0`). The fix is the
    /// breakdown, not a redefinition: keep the walked total, publish the source subset beside it, and let
    /// `files - source_files` name the docs/data/asset remainder.
    ///
    /// Mode A/B envelope ingest sets this equal to `files`: an envelope carries only files its adapter
    /// declared, so every one of them is parsed source by construction.
    pub source_files: usize,
    /// Symbols extracted across the tree.
    pub symbols: usize,
    /// Resolved dep-graph edges (sum of out-degrees).
    pub import_edges: usize,
    /// io provides (all kinds).
    pub io_provides: usize,
    /// io consumes with a resolved `key` (all kinds).
    pub io_consumes_keyed: usize,
    /// io consumes with `key: None` — recognized call site the adapter could not statically resolve.
    pub io_consumes_unresolved: usize,
    /// Files that degraded to a lexical fallback.
    pub degraded: usize,
    /// FACT, not a heuristic: this tree contributed NO JOINABLE io to the join (`io_provides == 0` AND
    /// `io_consumes_keyed == 0`) while it DID analyze files (`files > 0`). Redefined 2026-07-17 (was
    /// `io_provides == 0 AND io_consumes_keyed == 0 AND io_consumes_unresolved == 0`): an unresolved
    /// consume proves the extractor SAW a call site, it just could not resolve the target key, so it can
    /// never join anything either way — counting it toward "contributed" under-fired the flag on a tree
    /// with 0 provides, 0 keyed consumes, and 1+ unresolved consumes, which is still fully join-blind. The
    /// mode-1 active-blindness signal: such a tree is invisible to the cross-layer join, so join findings
    /// that reference it are structurally weak. Renderers turn this bool into the human "blind/dark"
    /// label (kernel stays fact-only). A pure UI library with no io legitimately trips this too — that
    /// over-disclosure is intentional (disclosure-only, never suppresses findings).
    ///
    /// EXACT zero (over `io_provides`/`io_consumes_keyed`) is deliberate and must NOT be "unified" with
    /// `framework_silence`'s near-zero floor (a pinned policy-value divergence, see that module's tests):
    /// this is an unconditional structural ASSERTION (always true when it fires), while the tripwires are
    /// heuristic self-reports that may fire at 1-2 extracted facts. Widening this to near-zero would turn
    /// the assertion into a heuristic.
    pub join_contribution_zero: bool,
}

impl CoverageCensus {
    /// Compute the census from the assembled `ir`, the visited `file_count`, the parser-claimed
    /// `source_file_count` (see [`source_files`](Self::source_files)), and the degraded-file count.
    /// Reads only `ir.ir.{dep, symbols, io}` — no re-parse, no vocabulary.
    pub fn compute(
        file_count: usize,
        source_file_count: usize,
        ir: &CommonIr,
        degraded: usize,
    ) -> CoverageCensus {
        let import_edges = ir.ir.dep.values().map(|targets| targets.len()).sum();
        let symbols = ir.ir.symbols.len();

        let (io_provides, io_consumes_keyed, io_consumes_unresolved) = match ir.ir.io.as_ref() {
            Some(io) => {
                let keyed = io.consumes.iter().filter(|c| c.key.is_some()).count();
                let unresolved = io.consumes.len() - keyed;
                (io.provides.len(), keyed, unresolved)
            }
            None => (0, 0, 0),
        };

        // "No JOINABLE contribution" (redefined 2026-07-17 — see `join_contribution_zero`'s doc):
        // unresolved consumes are deliberately EXCLUDED from this gate — they can never join anything
        // either way, so a tree with 0 provides, 0 keyed consumes, and 1+ unresolved consumes is still
        // fully join-blind, not "contributed something."
        let join_contribution_zero = file_count > 0 && io_provides == 0 && io_consumes_keyed == 0;

        CoverageCensus {
            files: file_count,
            source_files: source_file_count,
            symbols,
            import_edges,
            io_provides,
            io_consumes_keyed,
            io_consumes_unresolved,
            degraded,
            join_contribution_zero,
        }
    }
}
