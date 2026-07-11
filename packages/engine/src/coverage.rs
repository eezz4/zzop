//! Structural coverage census — a vocab-free, per-tree count of which analysis channels this tree
//! actually filled. It is a pure post-aggregate of the already-assembled `AnalyzeOutput` data (io / dep /
//! symbols / degraded / file_count): "key present means it ran", so a consumer can tell "analyzed and
//! found 0" apart from "this channel was dark". `join_contribution_zero` is the active-blindness FACT
//! (not a heuristic): a tree that contributed NO io to the cross-layer join. See
//! decision doc coverage-disclosure.md (Stage 1).

use zzop_core::CommonIr;

/// Vocab-free per-tree channel-fill census. All counts are kind-agnostic (every io kind, not just http).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoverageCensus {
    /// Files the walk visited (== `AnalyzeOutput::file_count`).
    pub files: usize,
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
    /// FACT, not a heuristic: this tree contributed NO io to the join (`io_provides == 0` AND both
    /// consume counts 0) while it DID analyze files (`files > 0`). The mode-1 active-blindness signal:
    /// such a tree is invisible to the cross-layer join, so join findings that reference it are
    /// structurally weak. Renderers turn this bool into the human "blind/dark" label (kernel stays fact-
    /// only). A pure UI library with no io legitimately trips this too — that over-disclosure is
    /// intentional (disclosure-only, never suppresses findings).
    ///
    /// EXACT zero is deliberate and must NOT be "unified" with `framework_silence`'s near-zero floor
    /// (a pinned policy-value divergence, see that module's tests): this is an unconditional structural
    /// ASSERTION (always true when it fires), while the tripwires are heuristic self-reports that may
    /// fire at 1-2 extracted facts. Widening this to near-zero would turn the assertion into a heuristic.
    pub join_contribution_zero: bool,
}

impl CoverageCensus {
    /// Compute the census from the assembled `ir`, the visited `file_count`, and the degraded-file count.
    /// Reads only `ir.ir.{dep, symbols, io}` — no re-parse, no vocabulary.
    pub fn compute(file_count: usize, ir: &CommonIr, degraded: usize) -> CoverageCensus {
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

        let join_contribution_zero = file_count > 0
            && io_provides == 0
            && io_consumes_keyed == 0
            && io_consumes_unresolved == 0;

        CoverageCensus {
            files: file_count,
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
