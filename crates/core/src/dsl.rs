//! Rule DSL — declarative rule definitions interpreted by the native engine. A rule pack is a JSON data file
//! (`rules/dsl/*.json`) the engine loads and interprets at runtime — the Biome GritQL / ast-grep / Semgrep
//! model. Complex whole-graph rules that the DSL cannot express stay as native rules (rules/native/*).
//!
//! ## Fused execution contract
//!
//! Per-file DSL rules run **in the parse pass**, before the file's AST is dropped: for each file the engine
//! parses, projects Common IR, runs the DSL rule packs against that file's slice, then drops the AST — one
//! pass, no re-read/re-parse. Raw AST is deliberately not part of this contract, so a rule sees only source
//! lines (`SourceFile::text`, for line-scan) and per-file spans (`SourceFile::symbols`, for method-scan). If
//! a parser falls back lexically and cannot produce spans, `symbols` is empty and method-scan silently
//! skips that file (line-scan still runs).
//!
//! Module layout: `def` (serde rule-pack types), `source` (interpreter input + minified detection),
//! `eval` (pack evaluation entry points), `prefilter` (RegexSet line-scan pre-filter), `markers`
//! (suppress-marker/require-file helpers), and one module per matcher family (`line_scan`,
//! `method_scan`, `ir_scan`). Every public item stays importable at `crate::dsl::X`.

mod def;
mod eval;
mod ir_scan;
mod line_scan;
mod markers;
mod method_scan;
mod prefilter;
mod source;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests_eval;
#[cfg(test)]
mod tests_http_conventions;
#[cfg(test)]
mod tests_ir_scan;
#[cfg(test)]
mod tests_line_scan;
#[cfg(test)]
mod tests_markers;
#[cfg(test)]
mod tests_method_scan;
#[cfg(test)]
mod tests_trigger_in_loop;

pub use def::{
    IoDirection, IoScan, LabeledPattern, LineScan, Matcher, MethodScan, RuleDef, RulePackDef,
    SymbolScan,
};
pub use eval::{eval_pack, eval_pack_profiled};
pub use source::{is_minified_or_generated, RuleContext, RuleTiming, SourceFile};
