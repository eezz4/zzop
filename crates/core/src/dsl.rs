//! Rule DSL — declarative rule definitions interpreted by the native engine. A rule pack is a JSON data file
//! (`rules/dsl/*.json`) the engine loads and interprets at runtime — the Biome GritQL / ast-grep / Semgrep
//! model. Complex whole-graph rules that the DSL cannot express stay as native rules (rules/native/*).
//!
//! ## Fused execution contract
//!
//! Per-file DSL rules (`LineScan`/`MethodScan`/`SymbolScan`/`CallScan`) run **in the parse pass**, before the file's
//! AST is dropped: for each file the engine parses, projects Common IR, runs the DSL rule packs against
//! that file's slice via `eval_pack`, then drops the AST — one pass, no re-read/re-parse. Raw AST is
//! deliberately not part of this contract, so a rule sees only source lines (`SourceFile::text`, for
//! line-scan) and per-file spans (`SourceFile::symbols`, for method-scan). If a parser falls back
//! lexically and cannot produce spans, `symbols` is empty and method-scan silently skips that file
//! (line-scan still runs).
//!
//! `IoScan` is the one exception, since the 2026 projection redesign: it evaluates WHOLE-TREE, via
//! [`eval_pack_io_scan`], called by the engine once after assemble — see that function's doc and
//! `ir_scan`'s module doc for why (assemble-composed provides and the tree-wide `AttributeStore` don't
//! exist yet inside the per-file pass). `eval_pack`'s own `Matcher::IoScan` dispatch arm is a no-op.
//!
//! ## Rule-skip visibility
//!
//! A rule whose pattern does not compile is SKIPPED, never fatal — one malformed rule must not fail the
//! run. Skipping SILENTLY, though, is the misleading-diagnosis failure this project treats as a cardinal
//! sin: the rule never fires and the run reads as clean. Every matcher therefore reports its skips into a
//! caller-owned `Vec<String>` (`diagnostics` module), reachable via the `*_into` entry points
//! ([`eval_pack_into`], [`eval_pack_profiled_into`], [`eval_pack_io_scan_into`]). The older
//! sink-less entry points still exist and still drop those messages — a caller that owns a warning
//! channel should call the `_into` twin.
//!
//! Module layout: `def` (serde rule-pack types), `fragments` (the `${NAME}` shared/reference mechanism
//! `RulePackDef::expand_fragments` uses), `source` (interpreter input + minified detection), `eval` (pack
//! evaluation entry points), `diagnostics` (rule-skip warning sink), `prefilter` (RegexSet line-scan
//! pre-filter), `markers`
//! (suppress-marker/require-file helpers), and one module per matcher family (`line_scan`,
//! `method_scan`, `call_scan`, `ir_scan`). Every public item stays importable at `crate::dsl::X`.

mod attr_gate;
mod call_scan;
mod def;
mod diagnostics;
mod eval;
mod fragments;
mod ir_scan;
mod line_scan;
mod literal_scan;
mod markers;
mod method_scan;
mod prefilter;
mod source;
mod string_mask;

#[cfg(test)]
mod inline_census_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests_call_scan;
#[cfg(test)]
mod tests_diagnostics;
#[cfg(test)]
mod tests_eval;
#[cfg(test)]
mod tests_fragments;
#[cfg(test)]
mod tests_http_conventions;
#[cfg(test)]
mod tests_ir_scan;
#[cfg(test)]
mod tests_line_scan;
#[cfg(test)]
mod tests_literal_scan;
#[cfg(test)]
mod tests_markers;
#[cfg(test)]
mod tests_method_scan;
#[cfg(test)]
mod tests_method_scan_after;
#[cfg(test)]
mod tests_method_scan_same_fn;
#[cfg(test)]
mod tests_test_regions;
#[cfg(test)]
mod tests_trigger_in_loop;

pub use attr_gate::apply_attr_gates;
pub use def::{
    CallScan, IoDirection, IoScan, LabeledPattern, LineScan, LiteralScan, Matcher, MethodScan,
    RuleDef, RulePackDef, SymbolScan,
};
pub use eval::{eval_pack, eval_pack_into, eval_pack_profiled, eval_pack_profiled_into};
pub use fragments::FragmentError;
pub use ir_scan::{eval_pack_io_scan, eval_pack_io_scan_into, IoScanTreeContext};
pub use markers::NEAR_MISS_MARKER_TOKEN_PATTERN;
pub use source::{is_minified_or_generated, RuleContext, RuleTiming, SourceFile};
