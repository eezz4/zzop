//! Envelope ingestion — the engine-side receiver for the external-parser Normalized AST protocol
//! (`docs/NORMALIZED_AST.md`). Projects a `zzop_core::NormalizedEnvelope`'s `FileProjection`s into the
//! same per-file shape `analyze::assemble` consumes, then runs the same whole-graph analyses
//! (dep-graph resolution, `circular`/`unreachable`/`dead-candidates`, `merge_findings`). An external
//! parser (Java/Python/JSP/anything this engine cannot parse natively) is therefore a first-class
//! citizen of every language-neutral analysis — the engine never sees the external parser's own AST,
//! only this projection.
//!
//! ## Deviations from the native per-file pass (documented, not bugs)
//!
//! - **No source text -> line-scan/method-scan DSL rules never run.** Those matchers scan source text
//!   directly; evaluating them against an empty string would silently look like "ran, found nothing"
//!   instead of "did not run". `SymbolScan`/`IoScan` only read `symbols`/`io`, which a `FileProjection`
//!   does supply, so `envelope_rule_pack` filters every pack down to just those two matcher kinds.
//!   Per-file lexical rules belong on the external parser's own side of the boundary.
//! - **No filesystem root -> no `dead-exports`/call-graph-BFS rules, no git-history analyses.** Those
//!   need a second disk read or a repository root, which an envelope has neither of; the affected
//!   `AnalyzeOutput` fields stay at their "git inactive" empty value, and a configured `git` option
//!   produces one `warnings` entry rather than a panic.
//! - **Dep-graph resolution treats import specifiers as repo-relative.** Edge resolution is a plain
//!   exact match against the envelope's own path set, not the TS parser's relative/extension-guessing
//!   resolver — an arbitrary external parser's `imports` map has no reason to follow TS conventions. An
//!   unmatched specifier is external, never an error; a `deferred` binding gets no edge (lazy import).
//!   [`resolve_envelope_specifier`] is a separate, narrower resolver used only for fragment
//!   `Ref`/`Mount` specifiers, which additionally understands `./`/`../` joins.
//! - **Fragment composition** (tRPC PROVIDEs, router-mount PROVIDEs) and late const-map CONSUME
//!   re-resolution run in envelope mode too, via the same composer functions the native path uses —
//!   only the resolver differs, since an envelope carries no tsconfig or workspace manifests to alias
//!   against.
//! - **No caching, no rule-timing profiling.** Both are ignored — envelope mode has no per-file disk
//!   content to hash and no per-rule timing loop wired for this smaller rule surface.
//!
//! ## Module layout
//!
//! Split by the two ingestion modes and their shared plumbing — every submodule is private, this root
//! re-exports the same three names the rest of the crate always consumed:
//!
//! - [`ingest`] — Mode A: `analyze_envelope`, the whole-tree envelope entry point (orchestrator).
//! - [`file_pass`] — Mode A's per-file accumulation loop, extracted verbatim from `analyze_envelope`.
//! - [`overlay`] — Mode B: `apply_adapter_overlays` + the shared fact-census predicate.
//! - [`merge`] — Mode B's two per-projection merge branches (existing artifact vs synthetic).
//! - [`reserved`] — reserved engine-internal io-sentinel predicates + drop/warning helpers, shared by
//!   both modes so they can't drift.
//! - [`resolve`] — the envelope-mode fragment-specifier resolver + the SymbolScan/IoScan pack filter.

mod file_pass;
mod ingest;
mod merge;
mod overlay;
mod reserved;
mod resolve;

#[cfg(test)]
mod tests;

pub use ingest::analyze_envelope;
pub(crate) use overlay::{apply_adapter_overlays, overlay_file_carries_facts};
