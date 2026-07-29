//! The regex preamble of an `IoScan` rule — every pattern compiled once, before any entry is walked.
//!
//! Split out of `super` on 2026-07-29 (line cap) along the seam that was already there: the evaluator has
//! two halves, "turn the rule's patterns into matchers or give up" and "walk the entries". The first half
//! is where the rule-skip contract lives (a rule whose regex does not compile is skipped, not fatal, and
//! never silent — see `super::super::diagnostics`), and keeping it whole makes that contract readable in
//! one place instead of interleaved with the matching loop.

use regex::Regex;

use super::super::def::{IoScan, RuleDef};
use super::super::diagnostics::RuleDiag;
use super::super::markers::compile_marker_line_comment;

/// Every matcher an [`eval_io_scan_rule`](super::eval_io_scan_rule) pass needs, plus the marker string the
/// suppress check renders. Constructed only when ALL of them compiled.
pub(super) struct IoScanPatterns {
    pub(super) file: Regex,
    pub(super) file_exclude: Option<Regex>,
    pub(super) key: Option<Regex>,
    pub(super) symbol: Option<Regex>,
    pub(super) anchor_exclude: Option<Regex>,
    pub(super) marker: String,
    pub(super) marker_re: Regex,
}

impl IoScanPatterns {
    /// Compiles the rule's patterns, or returns `None` after recording WHY through `diag` — the caller
    /// then skips this rule. Never panics and never silently drops: every `None` path has written a
    /// diagnostic first.
    pub(super) fn compile(rule: &RuleDef, m: &IoScan, diag: &mut RuleDiag) -> Option<Self> {
        let file = diag.compile("file_pattern", &m.file_pattern)?;
        let file_exclude =
            diag.compile_opt("file_exclude_pattern", m.file_exclude_pattern.as_ref())?;
        let key = diag.compile_opt("key_pattern", m.key_pattern.as_ref())?;
        let symbol = diag.compile_opt("symbol_pattern", m.symbol_pattern.as_ref())?;
        let anchor_exclude =
            diag.compile_opt("anchor_exclude_pattern", m.anchor_exclude_pattern.as_ref())?;
        // Line-comment-NEUTRAL marker (`//` or `#`) — io-scan anchor lines span every provide-producing
        // language, Python included, unlike the `//`-only per-file line/method-scan marker. Built from
        // the rule id (escaped), so a failure is structural rather than an author's bad pattern.
        let marker = rule.suppress_marker();
        let Some(marker_re) = compile_marker_line_comment(&marker) else {
            diag.malformed("its derived suppress marker does not compile as a regex");
            return None;
        };
        Some(Self {
            file,
            file_exclude,
            key,
            symbol,
            anchor_exclude,
            marker,
            marker_re,
        })
    }
}
