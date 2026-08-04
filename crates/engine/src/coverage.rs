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
    /// lockfiles and binary assets included. Use [`parser_dispatched`](Self::parser_dispatched) to size the code.
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
    /// `files - parser_dispatched` name the docs/data/asset remainder.
    ///
    /// RENAMED from `source_files` (wire: `sourceFiles`) on 2026-07-31, user ruling. The old name
    /// read as "files that are source code", and `.sql` IS source code — yet it sat outside the
    /// extension table's `structural` column (parser-sql projects io facts only, no symbols/imports
    /// by design), so a reader summing the table could never reproduce this number and had no way to
    /// tell why. The new name states the membership rule instead of a judgment: a PARSER DISPATCHED
    /// on the file. What the file's projection then contains is the extension table's axis, not this
    /// one's.
    ///
    /// Mode A/B envelope ingest sets this equal to `files`: an envelope carries only files its adapter
    /// declared, so every one of them is parsed source by construction.
    pub parser_dispatched: usize,
    /// Symbols extracted across the tree.
    pub symbols: usize,
    /// Sum of the dep graph's out-degrees over RESOLVED IN-TREE edges: one count per
    /// `(importing file, imported file)` pair the resolver mapped to a file this walk visited. An import
    /// of a published package and a specifier no resolver could map are both EXCLUDED — they never enter
    /// `ir.dep`, so they cannot be summed here. Edge count, not importing-file count.
    ///
    /// RENAMED from `import_edges` (wire: `importEdges`) on 2026-07-31, user ruling — the same standard
    /// [`parser_dispatched`](Self::parser_dispatched) was renamed under: a wire field's NAME must state
    /// its membership RULE, not a judgment the computation does not make. The old name claimed the
    /// tree's imports; the number is the tree's RESOLVED imports, and nothing on the wire said so. The
    /// founding misread: a 91-file Python tree reported `importEdges: 3` and was read as "this repo
    /// barely imports anything" when what it said was "3 of its imports landed on files in this tree" —
    /// the rest were `import requests`-shaped package imports, dropped during dep resolution. The
    /// channel was NOT widened to fix the misread (package imports are a different fact with a
    /// different key space, and they already ride `AnalyzeOutput::package_imports`); the name was
    /// narrowed to match what the channel has always carried. See
    /// [`zzop_core::DEP_GRAPH_RESOLVED_ONLY`] — the one owner of that rule's sentence, which the
    /// graph-derived metrics (`fanIn`/`fanOut`/`degree`/`blastRadius`) disclose instead of renaming,
    /// since those terms are correct ABOUT the graph they describe.
    pub resolved_import_edges: usize,
    /// F4 declared-import denominator for [`resolved_import_edges`](Self::resolved_import_edges): per
    /// extension (the facade coverage table's own lowercased-tail grain), the sum over that extension's
    /// parsed files of each file's DISTINCT declared import specifiers — import bindings, re-exports and
    /// dynamic `import()`s, counted BEFORE resolution drops the ones that fail. Definition, the
    /// measured/unmeasured contract (an ABSENT key means "never measured" — a channel-less parser like
    /// prisma/sql, an SFC, or a lexical-only file — never 0), and the deliberate non-1:1 with the edge
    /// count all live in `analyze::assemble::declared`'s docs; this field only carries the result.
    ///
    /// NOT set by [`compute`](Self::compute) (which reads only the IR, where declared specifiers no
    /// longer exist): the native assemble path stamps it after `compute` returns. The Mode A envelope
    /// path never stamps it, so an envelope-ingested tree carries an EMPTY map — every extension reads
    /// as unmeasured there, which is the honest value (nothing on that path counts declarations).
    pub declared_imports_by_ext: std::collections::BTreeMap<String, usize>,
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
    /// `parser_dispatched_count` (see [`parser_dispatched`](Self::parser_dispatched)), and the
    /// degraded-file count. Reads only `ir.ir.{dep, symbols, io}` — no re-parse, no vocabulary.
    pub fn compute(
        file_count: usize,
        parser_dispatched_count: usize,
        ir: &CommonIr,
        degraded: usize,
    ) -> CoverageCensus {
        let resolved_import_edges = ir.ir.dep.values().map(|targets| targets.len()).sum();
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
            parser_dispatched: parser_dispatched_count,
            symbols,
            resolved_import_edges,
            // Empty here by contract (see the field doc): declared counts are not derivable from `ir`,
            // and only the native assemble path stamps them after this returns.
            declared_imports_by_ext: std::collections::BTreeMap::new(),
            io_provides,
            io_consumes_keyed,
            io_consumes_unresolved,
            degraded,
            join_contribution_zero,
        }
    }
}
