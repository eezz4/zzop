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

use serde::{Deserialize, Serialize};

use crate::{
    finding::Finding,
    io::{IoFacts, IoKind},
    ir::CommonIr,
    ir::{SourceSymbol, SourceSymbolKind},
    Severity,
};

/// Rule interpreter input — source files (lexical rules) + optional Common IR (IR-query rules, later).
pub struct RuleContext<'a> {
    pub files: &'a [SourceFile],
    pub ir: Option<&'a CommonIr>,
}

/// Per-rule wall-clock timing from one `eval_pack_profiled` call — the substrate for rule profiling.
/// `rule_id` is pack-prefixed (`"{pack.id}/{rule.id}"`). `nanos` varies run-to-run with timer noise, so
/// rank rules by relative cost rather than diffing raw values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleTiming {
    pub rule_id: String,
    pub nanos: u128,
    pub findings: usize,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Normalized relative path.
    pub rel: String,
    pub text: String,
    /// Per-file symbol spans (functions/methods/classes), consumed by `Matcher::MethodScan`. Empty when
    /// the parser has no support / falls back lexically; line-scan ignores this field.
    pub symbols: Vec<SourceSymbol>,
    /// Per-file IO facts (`Matcher::IoScan`'s substrate), projected alongside `symbols`. `None` when the
    /// parser has no IO adapter / falls back lexically — io-scan rules silently skip such files.
    pub io: Option<IoFacts>,
    /// Per-file loop-body line spans (1-based, inclusive), projected alongside `symbols`: each
    /// `for`/`for-of`/`for-in`/`while`/`do-while` statement's full span (header line included — a call in
    /// the loop CONDITION runs once per iteration too), plus the span of the callback ARGUMENT of an
    /// array-iteration call (`.map`/`.forEach`/`.filter`/`.reduce`/...) — the callback only, not the whole
    /// call expression, so a receiver like `(await fetch(u)).items.map(...)` does not put the one-shot
    /// `fetch` "inside" the loop. Consumed by `MethodScan::trigger_in_loop`. Empty when the parser has no
    /// support / falls back lexically — structural rules silently skip such files (graceful degrade,
    /// same policy as `symbols`).
    pub loop_spans: Vec<(u32, u32)>,
}

/// A file is "minified/generated" iff EITHER prong holds:
///
/// 1. **Absolute prong**: any single line is 5000+ bytes long — never hand-written, regardless of how
///    small a fraction of the file it is.
/// 2. **Ratio prong**: any line is 500+ bytes long AND 500+ byte lines account for at least 50% of the
///    file's total bytes — long lines DOMINATE, the signature of bundler/generated output.
///
/// The ratio prong exists because a plain "any 500+ char line" rule causes collateral damage: an ordinary
/// hand-written file can happen to have one long comment or string literal among hundreds of normal
/// lines, and flagging on that alone would silently drop its entire DSL coverage.
///
/// Computed once per file. When true, the engine skips ALL DSL rule-pack evaluation for the file; native
/// structural extraction (symbols/imports/IO) is unaffected.
pub fn is_minified_or_generated(text: &str) -> bool {
    const LONG_LINE: usize = 500;
    const BLOB_LINE: usize = 5000;
    let mut total_bytes: usize = 0;
    let mut long_line_bytes: usize = 0;
    let mut has_long_line = false;
    for line in text.split('\n') {
        let len = line.len();
        total_bytes += len;
        if len >= BLOB_LINE {
            return true;
        }
        if len >= LONG_LINE {
            has_long_line = true;
            long_line_bytes += len;
        }
    }
    // Ratio prong: long lines must dominate (>= 50% of total bytes). `total_bytes == 0` (empty file)
    // never reaches a `true` here: `has_long_line` is false. Integer math, no float.
    has_long_line && long_line_bytes * 2 >= total_bytes
}

#[cfg(test)]
mod minified_tests {
    use super::is_minified_or_generated;

    #[test]
    fn normal_short_line_file_is_not_minified() {
        let text = "const x = 1;\nfunction f() {\n  return x;\n}\n";
        assert!(!is_minified_or_generated(text));
    }

    #[test]
    fn a_single_long_line_dominating_a_tiny_file_is_minified() {
        let text = format!(
            "const short = 1;\nconst bundled = \"{}\";\n",
            "x".repeat(600)
        );
        assert!(is_minified_or_generated(&text));
    }

    #[test]
    fn one_long_comment_line_inside_a_large_normal_file_is_not_minified() {
        let long_comment = format!("// {}", "word ".repeat(114)); // 573 bytes, >= 500
        assert!(long_comment.len() >= 500 && long_comment.len() < 600);
        let normal_line = "const someOrdinaryVariable = computeSomething();"; // ~49 bytes
        let mut text = String::new();
        for _ in 0..50 {
            text.push_str(normal_line);
            text.push('\n');
        }
        text.push_str(&long_comment);
        text.push('\n');
        for _ in 0..50 {
            text.push_str(normal_line);
            text.push('\n');
        }
        assert!(
            !is_minified_or_generated(&text),
            "one long comment line among 100 normal lines must not classify the file as minified"
        );
    }

    #[test]
    fn a_5000_char_blob_line_inside_a_large_normal_file_is_minified() {
        // The absolute prong fires even though the ratio prong alone would not (~5000 long-line bytes vs
        // ~14700 normal bytes is well under 50% dominance).
        let blob = "x".repeat(5000);
        let normal_line = "const someOrdinaryVariable = computeSomething();";
        let mut text = String::new();
        for _ in 0..150 {
            text.push_str(normal_line);
            text.push('\n');
        }
        text.push_str(&blob);
        text.push('\n');
        for _ in 0..150 {
            text.push_str(normal_line);
            text.push('\n');
        }
        assert!(is_minified_or_generated(&text));
    }

    #[test]
    fn a_499_char_line_is_the_boundary_and_is_not_minified() {
        let line = "x".repeat(499);
        assert_eq!(line.len(), 499);
        let text = format!("{line}\n");
        assert!(!is_minified_or_generated(&text));
    }

    #[test]
    fn a_500_char_line_that_dominates_is_the_boundary_and_is_minified() {
        let line = "x".repeat(500);
        assert_eq!(line.len(), 500);
        let text = format!("{line}\n");
        assert!(is_minified_or_generated(&text));
    }

    #[test]
    fn a_trailing_carriage_return_near_the_boundary_still_counts_correctly() {
        // `split('\n')` leaves a trailing `\r` on each line, so a line whose visible content is exactly
        // 499 chars becomes 500 bytes once its `\r` is counted, tripping the threshold a character
        // earlier than LF source would.
        let visible = "x".repeat(499);
        let text = format!("{visible}\r\n");
        assert!(
            is_minified_or_generated(&text),
            "a 499-char line plus a trailing \\r from CRLF must reach the 500-byte threshold"
        );
    }

    #[test]
    fn an_empty_file_is_not_minified() {
        assert!(!is_minified_or_generated(""));
    }
}

/// A rule pack (DSL) — maps to one `rules/dsl/<id>.json`. Independently shipped and versioned.
#[derive(Debug, Clone, Deserialize)]
pub struct RulePackDef {
    pub id: String,
    #[serde(default = "any_framework")]
    pub framework: String,
    /// DSL schema version this pack was authored against (see `docs/rules/dsl-reference.md`). Defaults to
    /// `1` when absent, so packs predating this field keep loading. `pack_loader::load_dsl_packs` rejects a
    /// pack whose version exceeds `pack_loader::SUPPORTED_DSL_SCHEMA_VERSION` as a mismatch, not new data to
    /// silently misinterpret; older-or-equal versions always load since schema evolution is additive-only.
    #[serde(default = "current_dsl_schema_version")]
    pub schema_version: u32,
    pub rules: Vec<RuleDef>,
}

fn any_framework() -> String {
    "any".into()
}

/// Default `RulePackDef::schema_version` for packs predating the field — always `1` (the oldest schema),
/// not `SUPPORTED_DSL_SCHEMA_VERSION`, even after that constant is bumped for a future schema revision.
fn current_dsl_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleDef {
    pub id: String,
    pub severity: Severity,
    /// Human-facing message (cause / fix hint).
    pub message: String,
    pub matcher: Matcher,
    /// Inline ok-marker suppression, applied uniformly to `LineScan` and `MethodScan` findings. A finding
    /// is suppressed when its own line, or the line directly above it (`MARKER_LOOKBACK_LINES`), contains a
    /// `//`-comment naming this marker (`// n+1-ok` or `// n+1-ok: reason` both suppress `suppress_marker:
    /// "n+1-ok"`). For a file whose extension is `.sql` (case-insensitive, see `is_sql_file`), a `--`-comment
    /// naming the marker suppresses identically (`-- n+1-ok`) — `--` is a line comment in SQL but not in
    /// JS/TS (`--x` is a decrement there), so this recognition is gated to `.sql` files only and never
    /// changes behavior for any other extension.
    #[serde(default)]
    pub suppress_marker: Option<String>,
}

/// Matcher — dispatched on the `type` tag. v0 was lexical line-scan + method-scan; symbol-scan and io-scan
/// (below) are the first IR-query matchers. Whole-graph queries (cross-file/cross-layer) still stay native.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Matcher {
    LineScan(LineScan),
    MethodScan(MethodScan),
    SymbolScan(SymbolScan),
    IoScan(IoScan),
}

/// Per-line regex scan.
/// Use either `line_pattern` (single) or `any` (labeled alternatives, first match per line wins).
#[derive(Debug, Clone, Deserialize)]
pub struct LineScan {
    /// Target file-path regex (e.g. `(?i)\.(java|jsp|jspx|tag)$`).
    pub file_pattern: String,
    /// Cheap pre-skip: only scan a file whose text contains this regex (if absent, always scan).
    pub require_file: Option<String>,
    /// Additional pre-skip regexes, ALL of which must match the file text, short-circuiting on first miss.
    /// Order cheapest/rarest-token-first to reject most files before an expensive probe runs.
    #[serde(default)]
    pub require_file_all: Vec<String>,
    /// Negated mirror of `require_file_all`: if **any** of these matches the whole file text, the rule
    /// skips that file entirely. Encodes "flag X only when there is no Y anywhere in the file" — a shape
    /// `exclude_pattern` can't express since it only vetoes the matching line, not the whole file.
    #[serde(default)]
    pub require_file_absent: Vec<String>,
    /// Skip lines whose trim_start begins with `//` `*` `/*` (comments).
    #[serde(default)]
    pub skip_comment_lines: bool,
    /// Regex that flags a line (single pattern, no label).
    #[serde(default)]
    pub line_pattern: Option<String>,
    /// Labeled alternatives — first match per line wins, label goes into `data.label`.
    #[serde(default)]
    pub any: Option<Vec<LabeledPattern>>,
    /// A line matching the main pattern is skipped when it ALSO matches this regex — e.g. excluding
    /// import-alias `as` from a type-safety `as`-cast counter.
    #[serde(default)]
    pub exclude_pattern: Option<String>,
    /// Optional path regex — a file whose `rel` path matches this is skipped entirely. `file_pattern` is
    /// positive-only and `regex` has no lookaround, so this is the escape hatch for "this extension but
    /// NOT under `scripts/`".
    #[serde(default)]
    pub file_exclude_pattern: Option<String>,
    /// Max snippet length (truncates long lines).
    #[serde(default = "default_snippet_max")]
    pub snippet_max: usize,
}

/// A regex + classification label (becomes a finding's `data.label` on first match).
#[derive(Debug, Clone, Deserialize)]
pub struct LabeledPattern {
    pub pattern: String,
    pub label: String,
}

fn default_snippet_max() -> usize {
    160
}

/// Multi-pattern co-occurrence within a symbol's body span (e.g. a command-injection detector requiring
/// `Runtime.exec`/`ProcessBuilder` to co-occur with string concatenation in the *same* method). Every
/// pattern in `patterns` must match somewhere in the span; `trigger` anchors the finding's line + snippet.
/// Spans come from `SourceFile.symbols`, projected by the parser; files with no symbols are skipped.
#[derive(Debug, Clone, Deserialize)]
pub struct MethodScan {
    /// Target file-path regex (e.g. `(?i)\.java$`).
    pub file_pattern: String,
    /// Cheap pre-skip: only scan a file whose text contains this regex (if absent, always scan).
    #[serde(default)]
    pub require_file: Option<String>,
    /// Additional pre-skip regexes, ALL of which must match the file text (see `LineScan::require_file_all`).
    #[serde(default)]
    pub require_file_all: Vec<String>,
    /// Negated mirror of `require_file_all` — see `LineScan::require_file_absent` (e.g. `process.exit(...)`
    /// with no `process.on('SIG...` signal-handling registration anywhere in the file).
    #[serde(default)]
    pub require_file_absent: Vec<String>,
    /// Skip lines whose trim_start begins with `//` `*` `/*` (comments) when testing any pattern.
    #[serde(default)]
    pub skip_comment_lines: bool,
    /// All of these must match somewhere within a symbol's body span for a finding.
    pub patterns: Vec<LabeledPattern>,
    /// `patterns[].label` whose first match (top-down) supplies the finding's line + snippet.
    pub trigger: String,
    /// Structural containment gate on the trigger pattern: when `true`, a trigger-pattern line match
    /// only counts (for both satisfaction and the finding's line) if it falls within one of the file's
    /// `SourceFile::loop_spans` — i.e. the call is textually INSIDE a loop statement or an
    /// array-iteration callback body, not merely co-occurring with loop tokens somewhere in the same
    /// function (the co-occurrence approximation behind the mono-hub 11/11 api-in-loop FP class).
    /// Non-trigger patterns are unaffected. A file with no projected loop spans (external parser,
    /// lexical fallback) can never satisfy the trigger, so the rule is silent there — graceful degrade,
    /// same policy as method-scan on a file with no symbol spans.
    #[serde(default)]
    pub trigger_in_loop: bool,
    /// After every `patterns` entry is satisfied, the finding is vetoed if ANY of these also matches a
    /// line in the SAME span — e.g. a try/catch guarding a TOCTOU race, or a `$transaction(...)` wrapper.
    #[serde(default)]
    pub absent: Vec<LabeledPattern>,
    /// Optional path regex — a file whose `rel` path matches this is skipped entirely. Same rationale as
    /// `LineScan::file_exclude_pattern`.
    #[serde(default)]
    pub file_exclude_pattern: Option<String>,
    /// Max snippet length (truncates long lines).
    #[serde(default = "default_snippet_max")]
    pub snippet_max: usize,
}

/// Query over a file's `SourceSymbol` list (declarations the parser projected), for naming-convention /
/// banned-export style rules line-scan can't express reliably (e.g. "every exported React component must
/// be PascalCase"). Filters combine with AND: `file_pattern` narrows the file set; `kind`/`name_pattern`/
/// `exported` narrow the symbols within it.
///
/// `negate` flips what `name_pattern` means rather than negating the whole matcher: `false` (default) fires
/// on a symbol matching it; `true` fires on a symbol NOT matching it. `negate: true` with no `name_pattern`
/// has nothing to negate against, so every symbol passes — equivalent to a plain `kind`/`exported` query.
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolScan {
    /// Target file-path regex (e.g. `(?i)\.tsx?$`).
    pub file_pattern: String,
    /// Restrict to one `SourceSymbolKind` (function/class/const/type/interface).
    #[serde(default)]
    pub kind: Option<SourceSymbolKind>,
    /// Regex on the symbol name — meaning flips under `negate` (see struct doc).
    #[serde(default)]
    pub name_pattern: Option<String>,
    /// Restrict to exported (`true`) or non-exported (`false`) symbols.
    #[serde(default)]
    pub exported: Option<bool>,
    /// See struct doc — flips `name_pattern`'s role from "must match" to "must not match".
    #[serde(default)]
    pub negate: bool,
}

/// Which side(s) of a file's `IoFacts` an `IoScan` rule queries.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IoDirection {
    Provides,
    Consumes,
    Any,
}

/// Query over a file's `IoFacts` (the cross-layer IO the parser projected alongside `symbols`), for
/// boundary-convention rules line-scan/method-scan can't express (e.g. "every HTTP endpoint must be
/// versioned under `/api/v[0-9]+/`"). Filters combine with AND: `direction` selects `provides`/`consumes`/
/// `any`, `kind` is an exact match. `key_pattern` + `negate` work like `SymbolScan`'s. An entry with
/// `key: None` (unresolved) never matches `key_pattern` — under `negate: true` that makes it a hit.
#[derive(Debug, Clone, Deserialize)]
pub struct IoScan {
    /// Target file-path regex — see struct doc for why this field is required.
    pub file_pattern: String,
    pub direction: IoDirection,
    /// Exact match against `IoProvide`/`IoConsume`'s `kind` string (e.g. `"http"`, `"db-table"`).
    #[serde(default)]
    pub kind: Option<IoKind>,
    /// Regex on the entry's normalized key — meaning flips under `negate` (see struct doc).
    #[serde(default)]
    pub key_pattern: Option<String>,
    /// See struct doc — flips `key_pattern`'s role from "must match" to "must not match".
    #[serde(default)]
    pub negate: bool,
}

/// A compiled per-line matcher — single or labeled alternatives.
enum LineMatch {
    Single(regex::Regex),
    Any(Vec<(regex::Regex, String)>),
}

/// Multi-pattern pre-filter for `Matcher::LineScan` (pure optimization). One `regex::RegexSet` is built
/// from every `LineScan` rule's patterns, each tagged with its owning rule's index — scanning a file's
/// lines through the set once yields exactly the rules with *any* chance of matching it.
struct LineScanPrefilter {
    set: regex::RegexSet,
    /// set-pattern-index -> owning rule's index in `pack.rules`.
    pattern_rule: Vec<usize>,
}

impl LineScanPrefilter {
    /// Build the set from `pack`. A `LineScan` rule with no compilable pattern contributes nothing. `None`
    /// if no valid pattern exists, or `RegexSet::new` errors — callers fall back to unfiltered evaluation.
    fn build(pack: &RulePackDef) -> Option<Self> {
        let mut patterns: Vec<String> = Vec::new();
        let mut pattern_rule: Vec<usize> = Vec::new();
        for (rule_idx, rule) in pack.rules.iter().enumerate() {
            let Matcher::LineScan(m) = &rule.matcher else {
                continue;
            };
            let rule_patterns: Vec<&str> = match (&m.any, &m.line_pattern) {
                (Some(alts), _) => {
                    let mut v = Vec::with_capacity(alts.len());
                    for lp in alts {
                        if regex::Regex::new(&lp.pattern).is_err() {
                            v.clear();
                            break; // one bad alt -> the whole rule contributes nothing (matches eval_line_scan)
                        }
                        v.push(lp.pattern.as_str());
                    }
                    v
                }
                (None, Some(p)) => {
                    if regex::Regex::new(p).is_ok() {
                        vec![p.as_str()]
                    } else {
                        vec![]
                    }
                }
                (None, None) => vec![],
            };
            for p in rule_patterns {
                patterns.push(p.to_string());
                pattern_rule.push(rule_idx);
            }
        }
        if patterns.is_empty() {
            return None;
        }
        let set = regex::RegexSet::new(&patterns).ok()?;
        Some(Self { set, pattern_rule })
    }

    /// `[rule_idx][file_idx] -> bool`: whether that rule has at least one set-pattern hit in that file.
    fn compute_candidates(&self, num_rules: usize, files: &[SourceFile]) -> Vec<Vec<bool>> {
        let mut matrix = vec![vec![false; files.len()]; num_rules];
        for (file_idx, f) in files.iter().enumerate() {
            for line in f.text.lines() {
                for pat_idx in self.set.matches(line).iter() {
                    matrix[self.pattern_rule[pat_idx]][file_idx] = true;
                }
            }
        }
        matrix
    }
}

/// Evaluate a whole rule pack -> findings.
pub fn eval_pack(pack: &RulePackDef, ctx: &RuleContext) -> Vec<Finding> {
    eval_pack_impl(pack, ctx, true, false).0
}

/// `eval_pack` with the `RegexSet` pre-filter forced off — the reference the differential test compares against.
#[cfg(test)]
fn eval_pack_no_prefilter(pack: &RulePackDef, ctx: &RuleContext) -> Vec<Finding> {
    eval_pack_impl(pack, ctx, false, false).0
}

/// Same as `eval_pack`, plus a `RuleTiming` per rule (wall time via `std::time::Instant`). Findings are
/// byte-for-byte identical to `eval_pack`'s, since this only adds timing around each rule's dispatch.
pub fn eval_pack_profiled(
    pack: &RulePackDef,
    ctx: &RuleContext,
) -> (Vec<Finding>, Vec<RuleTiming>) {
    eval_pack_impl(pack, ctx, true, true)
}

fn eval_pack_impl(
    pack: &RulePackDef,
    ctx: &RuleContext,
    use_prefilter: bool,
    profile: bool,
) -> (Vec<Finding>, Vec<RuleTiming>) {
    let mut out = Vec::new();
    let mut timings = Vec::new();
    let prefilter = use_prefilter
        .then(|| LineScanPrefilter::build(pack))
        .flatten();
    let candidates = prefilter
        .as_ref()
        .map(|p| p.compute_candidates(pack.rules.len(), ctx.files));
    for (rule_idx, rule) in pack.rules.iter().enumerate() {
        let start_len = out.len();
        let t0 = profile.then(std::time::Instant::now);
        match &rule.matcher {
            Matcher::LineScan(m) => {
                let file_candidates = candidates.as_ref().map(|c| c[rule_idx].as_slice());
                eval_line_scan(&pack.id, rule, m, ctx, file_candidates, &mut out);
            }
            Matcher::MethodScan(m) => eval_method_scan(&pack.id, rule, m, ctx, &mut out),
            Matcher::SymbolScan(m) => eval_symbol_scan(&pack.id, rule, m, ctx, &mut out),
            Matcher::IoScan(m) => eval_io_scan(&pack.id, rule, m, ctx, &mut out),
        }
        if let Some(t0) = t0 {
            timings.push(RuleTiming {
                rule_id: format!("{}/{}", pack.id, rule.id),
                nanos: t0.elapsed().as_nanos(),
                findings: out.len() - start_len,
            });
        }
    }
    (out, timings)
}

fn eval_line_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &LineScan,
    ctx: &RuleContext,
    // `Some(cand)` is the RegexSet pre-filter's per-file candidacy for this rule; `None` means the
    // pre-filter is disabled (scan every file).
    file_candidates: Option<&[bool]>,
    out: &mut Vec<Finding>,
) {
    // Skip the rule if a regex fails to compile (TODO: report via diagnostics).
    let Ok(file_re) = regex::Regex::new(&m.file_pattern) else {
        return;
    };
    // Path-negation escape hatch — see `LineScan::file_exclude_pattern` doc.
    let file_exclude_re = match &m.file_exclude_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let require_re = match &m.require_file {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let Some(require_all) = compile_require_all(&m.require_file_all) else {
        return;
    };
    // Negated mirror of require_file_all — see `LineScan::require_file_absent` doc. Reuses
    // `compile_require_all`; ANY-vs-ALL semantics are applied by the caller below.
    let Some(require_absent) = compile_require_all(&m.require_file_absent) else {
        return;
    };
    let exclude_re = match &m.exclude_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let marker_re = match &rule.suppress_marker {
        Some(marker) => match compile_marker(marker) {
            Some(r) => Some(r),
            None => return,
        },
        None => None,
    };
    // SQL-comment counterpart of `marker_re`, only ever consulted below when `is_sql_file(&f.rel)` — see
    // `compile_marker_sql`'s doc.
    let marker_re_sql = match &rule.suppress_marker {
        Some(marker) => match compile_marker_sql(marker) {
            Some(r) => Some(r),
            None => return,
        },
        None => None,
    };
    // `any` (labeled alternatives) takes precedence, else `line_pattern` (single). Neither -> invalid DSL -> skip.
    let matcher = match (&m.any, &m.line_pattern) {
        (Some(alts), _) => {
            let mut v = Vec::with_capacity(alts.len());
            for lp in alts {
                let Ok(re) = regex::Regex::new(&lp.pattern) else {
                    return;
                };
                v.push((re, lp.label.clone()));
            }
            LineMatch::Any(v)
        }
        (None, Some(p)) => match regex::Regex::new(p) {
            Ok(re) => LineMatch::Single(re),
            Err(_) => return,
        },
        (None, None) => return,
    };
    let rule_id = format!("{}/{}", pack_id, rule.id);

    for (file_idx, f) in ctx.files.iter().enumerate() {
        if let Some(cand) = file_candidates {
            if !cand[file_idx] {
                continue; // RegexSet proved zero pattern hits in this file — see fn doc
            }
        }
        if !file_re.is_match(&f.rel) {
            continue;
        }
        if let Some(re) = &file_exclude_re {
            if re.is_match(&f.rel) {
                continue; // path-negation escape hatch, see field doc
            }
        }
        if let Some(req) = &require_re {
            if !req.is_match(&f.text) {
                continue;
            }
        }
        if !require_all.iter().all(|re| re.is_match(&f.text)) {
            continue; // short-circuits on the first miss
        }
        if require_absent.iter().any(|re| re.is_match(&f.text)) {
            continue; // ANY match anywhere in the file skips it (require_file_absent)
        }
        let lines: Vec<&str> = f.text.lines().collect();
        let is_sql = is_sql_file(&f.rel);
        for (i, line) in lines.iter().enumerate() {
            if m.skip_comment_lines {
                let t = line.trim_start();
                if t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") {
                    continue;
                }
            }
            if let Some(re) = &exclude_re {
                if re.is_match(line) {
                    continue;
                }
            }
            let label: Option<&str> = match &matcher {
                LineMatch::Single(re) => {
                    if re.is_match(line) {
                        Some("")
                    } else {
                        None
                    }
                }
                LineMatch::Any(alts) => alts
                    .iter()
                    .find(|(re, _)| re.is_match(line))
                    .map(|(_, label)| label.as_str()),
            };
            let Some(label) = label else { continue };
            if let Some(re) = &marker_re {
                if marker_suppresses(re, &lines, i) {
                    continue;
                }
            }
            if is_sql {
                if let Some(re) = &marker_re_sql {
                    if marker_suppresses(re, &lines, i) {
                        continue;
                    }
                }
            }
            let snippet: String = line.trim().chars().take(m.snippet_max).collect();
            let data = if label.is_empty() {
                serde_json::json!({ "snippet": snippet })
            } else {
                serde_json::json!({ "snippet": snippet, "label": label })
            };
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: (i + 1) as u32,
                message: rule.message.clone(),
                data: Some(data),
            });
        }
    }
}

/// Builds the regex for `RuleDef::suppress_marker` — matches a `//` comment naming the marker (regex-escaped
/// so metacharacters like `n+1-ok`'s `+` match literally), optionally followed by `:` and free text.
fn compile_marker(marker: &str) -> Option<regex::Regex> {
    regex::Regex::new(&format!(r"//\s*{}\b", regex::escape(marker))).ok()
}

/// Builds the SQL-comment counterpart of `compile_marker` — matches a `--` comment naming the marker,
/// same escaping/suffix rules. Only ever consulted for `.sql` files (see `is_sql_file`); `--` is not a
/// comment marker in JS/TS (`--x` is a decrement there), so this regex must never be applied outside SQL.
fn compile_marker_sql(marker: &str) -> Option<regex::Regex> {
    regex::Regex::new(&format!(r"--\s*{}\b", regex::escape(marker))).ok()
}

/// Whether `--`-comment suppress markers should be recognized for this file — gated on the `.sql`
/// extension (case-insensitive) so `//`-only recognition stays byte-identical for every other extension.
fn is_sql_file(rel: &str) -> bool {
    std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sql"))
}

/// Compiles `require_file_all`. `None` when any pattern fails to compile, skipping the whole rule.
fn compile_require_all(patterns: &[String]) -> Option<Vec<regex::Regex>> {
    patterns.iter().map(|p| regex::Regex::new(p).ok()).collect()
}

/// How far above a finding a `// <marker>-ok` comment still suppresses it, one uniform window across every
/// rule. Set to 1: a wider window risks a marker aimed at one call silently suppressing unrelated sibling
/// findings a few lines below it. Bump `zzop-engine`'s `DSL_INTERPRETER_FINGERPRINT` when changing this.
const MARKER_LOOKBACK_LINES: usize = 1;

/// Whether the marker comment appears on the finding's own line or within `MARKER_LOOKBACK_LINES` above it.
fn marker_suppresses(re: &regex::Regex, lines: &[&str], line_idx: usize) -> bool {
    (line_idx.saturating_sub(MARKER_LOOKBACK_LINES)..=line_idx)
        .any(|i| lines.get(i).is_some_and(|l| re.is_match(l)))
}

fn eval_method_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &MethodScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
) {
    // Skip the rule if a regex fails to compile (TODO: report via diagnostics).
    let Ok(file_re) = regex::Regex::new(&m.file_pattern) else {
        return;
    };
    // Path-negation escape hatch — see `LineScan::file_exclude_pattern` doc.
    let file_exclude_re = match &m.file_exclude_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let require_re = match &m.require_file {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let mut patterns = Vec::with_capacity(m.patterns.len());
    for lp in &m.patterns {
        let Ok(re) = regex::Regex::new(&lp.pattern) else {
            return;
        };
        patterns.push((re, lp.label.clone()));
    }
    // The trigger label must be one of `patterns` — otherwise the DSL rule is malformed, skip it.
    let Some(trigger_idx) = patterns.iter().position(|(_, label)| *label == m.trigger) else {
        return;
    };
    // Veto patterns (guard present -> not a violation) — compiled like `patterns` above.
    let mut absent = Vec::with_capacity(m.absent.len());
    for lp in &m.absent {
        let Ok(re) = regex::Regex::new(&lp.pattern) else {
            return;
        };
        absent.push(re);
    }
    let marker_re = match &rule.suppress_marker {
        Some(marker) => match compile_marker(marker) {
            Some(r) => Some(r),
            None => return,
        },
        None => None,
    };
    // SQL-comment counterpart of `marker_re`, only ever consulted below when `is_sql_file(&f.rel)` — see
    // `compile_marker_sql`'s doc.
    let marker_re_sql = match &rule.suppress_marker {
        Some(marker) => match compile_marker_sql(marker) {
            Some(r) => Some(r),
            None => return,
        },
        None => None,
    };
    let Some(require_all) = compile_require_all(&m.require_file_all) else {
        return;
    };
    // Negated mirror of require_file_all, see `MethodScan::require_file_absent` doc.
    let Some(require_absent) = compile_require_all(&m.require_file_absent) else {
        return;
    };
    let rule_id = format!("{}/{}", pack_id, rule.id);

    for f in ctx.files {
        if !file_re.is_match(&f.rel) {
            continue;
        }
        if let Some(re) = &file_exclude_re {
            if re.is_match(&f.rel) {
                continue; // path-negation escape hatch, see field doc
            }
        }
        if let Some(req) = &require_re {
            if !req.is_match(&f.text) {
                continue;
            }
        }
        if !require_all.iter().all(|re| re.is_match(&f.text)) {
            continue; // short-circuits on the first miss
        }
        if require_absent.iter().any(|re| re.is_match(&f.text)) {
            continue; // ANY match anywhere in the file skips it (require_file_absent)
        }
        // Whole-file necessary-condition pre-skip: every `patterns` entry must match SOMEWHERE in the file,
        // a strict subsumption of the per-span check below, so findings stay identical.
        if !patterns.iter().all(|(re, _)| re.is_match(&f.text)) {
            continue;
        }
        let lines: Vec<&str> = f.text.lines().collect();
        let is_sql = is_sql_file(&f.rel);
        // Innermost-span priority: when spans overlap (a class symbol's span contains its methods' spans),
        // drop any symbol whose span strictly contains another candidate span — avoids double-counting.
        let spans: Vec<(usize, u32, u32)> = f
            .symbols
            .iter()
            .enumerate()
            .filter_map(|(idx, sym)| {
                let (Some(s), Some(e)) = (sym.body_start, sym.body_end) else {
                    return None;
                };
                (s != 0 && e >= s).then_some((idx, s, e))
            })
            .collect();
        let mut drop_symbol = vec![false; f.symbols.len()];
        for &(idx_a, s_a, e_a) in &spans {
            for &(idx_b, s_b, e_b) in &spans {
                if idx_a != idx_b && s_a <= s_b && e_a >= e_b && (s_a, e_a) != (s_b, e_b) {
                    drop_symbol[idx_a] = true;
                    break;
                }
            }
        }

        for (sym_idx, sym) in f.symbols.iter().enumerate() {
            if drop_symbol[sym_idx] {
                continue; // outer span strictly contains another candidate span — evaluate the leaf instead
            }
            let (Some(body_start), Some(body_end)) = (sym.body_start, sym.body_end) else {
                continue; // no body span (type/interface, or parser couldn't project one) -> not scannable
            };
            if body_start == 0 || body_end < body_start {
                continue; // malformed span, defensively skip
            }
            let start_idx = (body_start - 1) as usize;
            if start_idx >= lines.len() {
                continue;
            }
            let end_idx = (body_end as usize).min(lines.len()); // exclusive; body_end is 1-based inclusive
            let span = &lines[start_idx..end_idx];

            let mut satisfied = vec![false; patterns.len()];
            let mut trigger_hit: Option<(usize, &str)> = None; // (index within span, line text)
            let mut vetoed = false;
            for (i, line) in span.iter().enumerate() {
                if m.skip_comment_lines {
                    let t = line.trim_start();
                    if t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") {
                        continue;
                    }
                }
                for (pi, (re, _)) in patterns.iter().enumerate() {
                    if !satisfied[pi] && re.is_match(line) {
                        if pi == trigger_idx && m.trigger_in_loop {
                            // Structural containment gate: this trigger match only counts if the
                            // line is textually inside a loop statement or array-iteration
                            // callback body — see `MethodScan::trigger_in_loop` and
                            // `SourceFile::loop_spans` docs. A match outside every loop span is a
                            // plain co-occurrence and neither satisfies the trigger nor can supply
                            // the finding's line.
                            let abs_line = body_start + i as u32;
                            if !f
                                .loop_spans
                                .iter()
                                .any(|&(s, e)| s <= abs_line && abs_line <= e)
                            {
                                continue;
                            }
                        }
                        satisfied[pi] = true;
                        if pi == trigger_idx && trigger_hit.is_none() {
                            trigger_hit = Some((i, line));
                        }
                    }
                }
                if !vetoed && absent.iter().any(|re| re.is_match(line)) {
                    vetoed = true;
                }
            }
            if vetoed || !satisfied.iter().all(|&b| b) {
                continue;
            }
            let Some((i, line)) = trigger_hit else {
                continue; // unreachable: satisfied[trigger_idx] implies trigger_hit is Some
            };
            if let Some(re) = &marker_re {
                if marker_suppresses(re, &lines, start_idx + i) {
                    continue;
                }
            }
            if is_sql {
                if let Some(re) = &marker_re_sql {
                    if marker_suppresses(re, &lines, start_idx + i) {
                        continue;
                    }
                }
            }
            let snippet: String = line.trim().chars().take(m.snippet_max).collect();
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: body_start + i as u32,
                message: rule.message.clone(),
                data: Some(serde_json::json!({ "snippet": snippet, "method": sym.name })),
            });
        }
    }
}

fn eval_symbol_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &SymbolScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
) {
    // Skip the rule if a regex fails to compile (TODO: report via diagnostics).
    let Ok(file_re) = regex::Regex::new(&m.file_pattern) else {
        return;
    };
    let name_re = match &m.name_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let rule_id = format!("{}/{}", pack_id, rule.id);

    for f in ctx.files {
        if !file_re.is_match(&f.rel) {
            continue;
        }
        for sym in &f.symbols {
            if let Some(k) = &m.kind {
                if sym.kind != *k {
                    continue;
                }
            }
            if let Some(exported) = m.exported {
                if sym.exported != exported {
                    continue;
                }
            }
            // `name_pattern`'s role flips under `negate` — see `SymbolScan`'s doc comment.
            let name_matches = name_re.as_ref().map(|re| re.is_match(&sym.name));
            let keep = match (m.negate, name_matches) {
                (false, None) => true,
                (false, Some(matched)) => matched,
                (true, None) => true,
                (true, Some(matched)) => !matched,
            };
            if !keep {
                continue;
            }
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: sym.line,
                message: rule.message.clone(),
                data: Some(serde_json::json!({ "snippet": sym.name })),
            });
        }
    }
}

fn eval_io_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &IoScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
) {
    // Skip the rule if a regex fails to compile (TODO: report via diagnostics).
    let Ok(file_re) = regex::Regex::new(&m.file_pattern) else {
        return;
    };
    let key_re = match &m.key_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let rule_id = format!("{}/{}", pack_id, rule.id);

    for f in ctx.files {
        if !file_re.is_match(&f.rel) {
            continue;
        }
        let Some(io) = &f.io else {
            continue; // no IO projection for this file (see SourceFile::io doc) -> nothing to scan
        };
        // One flattened list of (kind, key, line) regardless of provide/consume — both line fields are
        // mandatory `u32`s today, so a "fall back to line 1" case is unreachable and not coded.
        let mut entries: Vec<(&str, Option<&str>, u32)> = Vec::new();
        if matches!(m.direction, IoDirection::Provides | IoDirection::Any) {
            entries.extend(
                io.provides
                    .iter()
                    .map(|p| (p.kind.as_str(), Some(p.key.as_str()), p.line)),
            );
        }
        if matches!(m.direction, IoDirection::Consumes | IoDirection::Any) {
            entries.extend(
                io.consumes
                    .iter()
                    .map(|c| (c.kind.as_str(), c.key.as_deref(), c.line)),
            );
        }
        for (kind, key, line) in entries {
            if let Some(k) = &m.kind {
                if kind != k.as_str() {
                    continue;
                }
            }
            // `key_pattern`'s role flips under `negate` — see `IoScan`'s doc; a key-less entry never matches.
            let matches_pattern = match (&key_re, key) {
                (Some(re), Some(k)) => re.is_match(k),
                (Some(_), None) => false,
                (None, _) => true,
            };
            let keep = if m.negate {
                !matches_pattern
            } else {
                matches_pattern
            };
            if !keep {
                continue;
            }
            let snippet = key.unwrap_or("<unresolved>").to_string();
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line,
                message: rule.message.clone(),
                data: Some(serde_json::json!({ "snippet": snippet, "kind": kind })),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    //! Exercises the full DSL pipeline end-to-end against the java-security rule pack.
    use super::*;

    fn pack() -> RulePackDef {
        serde_json::from_str(include_str!(
            "../../../rules/dsl/java-security/java-security.json"
        ))
        .expect("parse java-security.json")
    }

    fn scan(src: &str, rel: &str) -> Vec<Finding> {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: rel.into(),
            text: src.into(),
            symbols: vec![],
            io: None,
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        eval_pack(&pack(), &ctx)
    }

    /// Builds a `(name, body_start, body_end)` method span into a `SourceSymbol` — hand-built here since
    /// Java isn't parsed in Rust yet; a real parser adapter would project these from its AST.
    fn method(name: &str, body_start: u32, body_end: u32) -> SourceSymbol {
        SourceSymbol {
            id: format!("C.java#{name}"),
            file: "C.java".into(),
            name: name.into(),
            kind: SourceSymbolKind::Function,
            line: body_start,
            exported: false,
            is_default: false,
            body_start: Some(body_start),
            body_end: Some(body_end),
            write_sites: Vec::new(),
        }
    }

    fn scan_methods(src: &str, symbols: Vec<SourceSymbol>) -> Vec<Finding> {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: "C.java".into(),
            text: src.into(),
            symbols,
            io: None,
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        eval_pack(&pack(), &ctx)
    }

    fn snippet(f: &Finding) -> String {
        f.data.as_ref().unwrap()["snippet"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn label(f: &Finding) -> String {
        f.data.as_ref().unwrap()["label"]
            .as_str()
            .unwrap()
            .to_string()
    }

    // --- sql-taint ---

    #[test]
    fn flags_sql_concatenated_with_variable() {
        let f = scan(
            r#"Query q = em.createQuery("SELECT u FROM User u WHERE u.login = '" + login + "'");"#,
            "C.java",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 1);
        assert!(snippet(&f[0]).contains("createQuery"));
    }

    #[test]
    fn does_not_flag_parameterized_query() {
        assert!(scan(
            r#"Query q = em.createQuery("SELECT u FROM User u WHERE u.login = :login");"#,
            "C.java",
        )
        .is_empty());
    }

    #[test]
    fn does_not_flag_constant_concatenation() {
        assert!(scan(r#"String s = "SELECT a " + "FROM b";"#, "C.java").is_empty());
    }

    #[test]
    fn does_not_flag_non_sql_concatenation() {
        assert!(scan(r#"log.info("user selected " + name);"#, "C.java").is_empty());
    }

    #[test]
    fn does_not_flag_lone_keyword_in_prose() {
        assert!(scan(
            r#"throw new IllegalArgumentException("Cannot merge with object of type [" + parent + "]");"#,
            "C.java",
        )
        .is_empty());
    }

    #[test]
    fn ignores_sql_in_comment() {
        assert!(scan(r#"// example: "SELECT * FROM t WHERE x=" + v"#, "C.java").is_empty());
    }

    #[test]
    fn works_on_jsp_scriptlets() {
        let f = scan(
            r#"<% String q = "SELECT * FROM t WHERE id=" + request.getParameter("id"); %>"#,
            "x.jsp",
        );
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn flags_delete_concatenation() {
        assert_eq!(
            scan(
                r#"st.executeUpdate("DELETE FROM t WHERE id=" + id);"#,
                "C.java"
            )
            .len(),
            1
        );
    }

    // --- weak-crypto ---

    #[test]
    fn flags_digestutils_md5() {
        let f = scan(
            r#"return DigestUtils.md5DigestAsHex(password.getBytes());"#,
            "C.java",
        );
        assert_eq!(f.len(), 1);
        assert!(label(&f[0]).contains("weak hash"));
    }

    #[test]
    fn flags_messagedigest_md5_and_sha1() {
        assert_eq!(
            scan(
                r#"MessageDigest md = MessageDigest.getInstance("MD5");"#,
                "C.java"
            )
            .len(),
            1
        );
        assert_eq!(
            scan(r#"MessageDigest.getInstance("SHA-1");"#, "C.java").len(),
            1
        );
    }

    #[test]
    fn flags_weak_ciphers_and_ecb() {
        let des = scan(r#"Cipher.getInstance("DES/CBC/PKCS5Padding");"#, "C.java");
        assert!(label(&des[0]).contains("weak cipher"));
        let ecb = scan(r#"Cipher.getInstance("AES/ECB/PKCS5Padding");"#, "C.java");
        assert!(label(&ecb[0]).contains("ECB"));
    }

    #[test]
    fn does_not_flag_strong_primitives() {
        assert!(scan(r#"MessageDigest.getInstance("SHA-256");"#, "C.java").is_empty());
        assert!(scan(r#"Cipher.getInstance("AES/GCM/NoPadding");"#, "C.java").is_empty());
        // 3DES is not single-DES
        assert!(scan(
            r#"Cipher.getInstance("DESede/CBC/PKCS5Padding");"#,
            "C.java"
        )
        .is_empty());
    }

    #[test]
    fn ignores_weak_crypto_in_comments() {
        assert!(scan(
            r#"// legacy used DigestUtils.md5DigestAsHex here"#,
            "C.java"
        )
        .is_empty());
    }

    // --- cmd-injection --- (no Java parser yet, so these tests hand-supply method spans via `SourceSymbol`)

    #[test]
    fn flags_method_that_execs_and_concatenates_dvja_pingaction_pattern() {
        let src = "public class C {\n  private void run() {\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}";
        let f = scan_methods(src, vec![method("run", 2, 5)]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].data.as_ref().unwrap()["method"], "run");
        assert!(snippet(&f[0]).contains("ping"));
    }

    #[test]
    fn does_not_flag_exec_with_constant_command_no_concatenation() {
        let src = "public class C { void r(){ Runtime.getRuntime().exec(\"ls -la\"); } }";
        assert!(scan_methods(src, vec![method("r", 1, 1)]).is_empty());
    }

    #[test]
    fn does_not_flag_string_concatenation_in_a_method_that_never_execs() {
        let src = "public class C { String g(String n){ return \"hello \" + n; } }";
        assert!(scan_methods(src, vec![method("g", 1, 1)]).is_empty());
    }

    #[test]
    fn does_not_pair_an_exec_in_one_method_with_a_concat_in_another_method_scoped() {
        let src = "public class C {\n  void a() { Runtime.getRuntime().exec(\"safe\"); }\n  String b(String x) { return \"msg \" + x; }\n}";
        let f = scan_methods(src, vec![method("a", 2, 2), method("b", 3, 3)]);
        assert!(f.is_empty());
    }

    #[test]
    fn processbuilder_plus_concatenation_is_flagged() {
        let src =
            "public class C { void r(String h){ new ProcessBuilder(\"sh\",\"-c\",\"curl \" + h).start(); } }";
        let f = scan_methods(src, vec![method("r", 1, 1)]);
        assert_eq!(f.len(), 1);
    }

    // --- symbol-scan (see module doc) ---

    fn symbol(name: &str, kind: SourceSymbolKind, line: u32, exported: bool) -> SourceSymbol {
        SourceSymbol {
            id: format!("f.ts#{name}"),
            file: "f.ts".into(),
            name: name.into(),
            kind,
            line,
            exported,
            is_default: false,
            body_start: None,
            body_end: None,
            write_sites: Vec::new(),
        }
    }

    fn symbol_scan_pack(matcher_json: &str) -> RulePackDef {
        let src = format!(
            r#"{{"id":"t","framework":"any","rules":[{{"id":"r","severity":"info","message":"m","matcher":{matcher_json}}}]}}"#
        );
        serde_json::from_str(&src).expect("parse inline symbol-scan pack")
    }

    fn scan_symbols(rel: &str, symbols: Vec<SourceSymbol>, matcher_json: &str) -> Vec<Finding> {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: rel.into(),
            text: String::new(),
            symbols,
            io: None,
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        eval_pack(&symbol_scan_pack(matcher_json), &ctx)
    }

    #[test]
    fn symbol_scan_non_negated_flags_names_matching_the_pattern() {
        let f = scan_symbols(
            "f.ts",
            vec![
                symbol("useFoo", SourceSymbolKind::Function, 3, true),
                symbol("bar", SourceSymbolKind::Function, 8, true),
            ],
            r#"{"type":"symbol-scan","file_pattern":"\\.ts$","name_pattern":"^use[A-Z]"}"#,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 3);
        assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "useFoo");
    }

    #[test]
    fn symbol_scan_negate_flags_exported_functions_not_matching_naming_convention() {
        let f = scan_symbols(
            "f.ts",
            vec![
                symbol("handleClick", SourceSymbolKind::Function, 1, true),
                symbol("onClick", SourceSymbolKind::Function, 5, true),
                symbol("helper", SourceSymbolKind::Function, 9, false), // not exported -> filtered out
            ],
            r#"{"type":"symbol-scan","file_pattern":"\\.ts$","kind":"function","exported":true,"name_pattern":"^handle[A-Z]","negate":true}"#,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 5);
        assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "onClick");
    }

    #[test]
    fn symbol_scan_kind_and_exported_filters_combine_with_and() {
        let f = scan_symbols(
            "f.ts",
            vec![
                symbol("Widget", SourceSymbolKind::Class, 1, true),
                symbol("Config", SourceSymbolKind::Type, 4, true), // wrong kind -> excluded
                symbol("widget", SourceSymbolKind::Class, 7, false), // not exported -> excluded
            ],
            r#"{"type":"symbol-scan","file_pattern":"\\.ts$","kind":"class","exported":true}"#,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 1);
    }

    #[test]
    fn symbol_scan_negate_with_no_name_pattern_behaves_as_plain_and_filter() {
        let f = scan_symbols(
            "f.ts",
            vec![
                symbol("a", SourceSymbolKind::Function, 1, true),
                symbol("b", SourceSymbolKind::Function, 2, false),
            ],
            r#"{"type":"symbol-scan","file_pattern":"\\.ts$","exported":true,"negate":true}"#,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 1);
    }

    // --- io-scan (see module doc) ---

    fn io_scan_pack(matcher_json: &str) -> RulePackDef {
        let src = format!(
            r#"{{"id":"t","framework":"any","rules":[{{"id":"r","severity":"info","message":"m","matcher":{matcher_json}}}]}}"#
        );
        serde_json::from_str(&src).expect("parse inline io-scan pack")
    }

    fn scan_io(rel: &str, io: IoFacts, matcher_json: &str) -> Vec<Finding> {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: rel.into(),
            text: String::new(),
            symbols: vec![],
            io: Some(io),
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        eval_pack(&io_scan_pack(matcher_json), &ctx)
    }

    fn io_provide(kind: &str, key: &str, line: u32) -> crate::io::IoProvide {
        crate::io::IoProvide {
            kind: kind.into(),
            key: key.into(),
            file: "f.ts".into(),
            line,
            symbol: None,
        }
    }

    fn io_consume(kind: &str, key: Option<&str>, line: u32) -> crate::io::IoConsume {
        crate::io::IoConsume {
            kind: kind.into(),
            key: key.map(Into::into),
            file: "f.ts".into(),
            line,
            raw: None,
            method: None,
        }
    }

    #[test]
    fn io_scan_negate_flags_provide_keys_not_matching_the_pattern() {
        let io = IoFacts {
            provides: vec![
                io_provide("http", "GET /authen/getUserInfo", 10),
                io_provide("http", "GET /api/v1/users", 20),
            ],
            consumes: vec![],
        };
        let f = scan_io(
            "f.ts",
            io,
            r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","kind":"http","key_pattern":"^GET /api/v[0-9]+/","negate":true}"#,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 10);
        assert_eq!(
            f[0].data.as_ref().unwrap()["snippet"],
            "GET /authen/getUserInfo"
        );
    }

    #[test]
    fn io_scan_key_none_never_matches_pattern_so_negate_flags_it() {
        let io = IoFacts {
            provides: vec![],
            consumes: vec![
                io_consume("http", None, 5), // unresolved dynamic target
                io_consume("http", Some("GET /api/v1/orders"), 6), // versioned, compliant
            ],
        };
        let f = scan_io(
            "f.ts",
            io,
            r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"consumes","kind":"http","key_pattern":"^GET /api/v[0-9]+/","negate":true}"#,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 5);
        assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "<unresolved>");
    }

    #[test]
    fn io_scan_kind_filter_excludes_non_matching_entries() {
        let io = IoFacts {
            provides: vec![
                io_provide("http", "GET /a", 1),
                io_provide("queue", "topic:jobs", 2),
            ],
            consumes: vec![],
        };
        let f = scan_io(
            "f.ts",
            io,
            r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","kind":"queue"}"#,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "topic:jobs");
    }

    #[test]
    fn io_scan_any_direction_scans_both_provides_and_consumes() {
        let io = IoFacts {
            provides: vec![io_provide("http", "GET /a", 1)],
            consumes: vec![io_consume("http", Some("GET /b"), 2)],
        };
        let f = scan_io(
            "f.ts",
            io,
            r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"any","kind":"http"}"#,
        );
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn io_scan_skips_files_with_no_io_projection() {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: "f.ts".into(),
            text: String::new(),
            symbols: vec![],
            io: None,
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let pack = io_scan_pack(r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"any"}"#);
        assert!(eval_pack(&pack, &ctx).is_empty());
    }

    // --- http-conventions (test-only matcher-shape demo, inlined rather than a shipped rules/dsl/*.json) ---

    const HTTP_CONVENTIONS_JSON: &str = r#"{
        "id": "http-conventions",
        "framework": "any",
        "schema_version": 1,
        "rules": [
            {
                "id": "endpoint-version-prefix",
                "severity": "warning",
                "message": "HTTP endpoint is not exposed under a versioned /api/v<N>/ prefix — add a version prefix so a future breaking change doesn't silently break existing callers.",
                "matcher": {
                    "type": "io-scan",
                    "file_pattern": "(?i)routes?/",
                    "direction": "provides",
                    "kind": "http",
                    "key_pattern": "^(?:GET|POST|PUT|PATCH|DELETE) /api/v[0-9]+/",
                    "negate": true
                }
            },
            {
                "id": "unversioned-fetch",
                "severity": "info",
                "message": "Fetches an HTTP endpoint that is not under a versioned /api/v<N>/ prefix, or whose target could not be statically resolved — confirm this call is intentionally unversioned.",
                "matcher": {
                    "type": "io-scan",
                    "file_pattern": "(?i)\\.(ts|tsx)$",
                    "direction": "consumes",
                    "kind": "http",
                    "key_pattern": "^(?:GET|POST|PUT|PATCH|DELETE) /api/v[0-9]+/",
                    "negate": true
                }
            },
            {
                "id": "exported-handler-naming",
                "severity": "warning",
                "message": "Exported route handler does not follow the handle<Name> naming convention — rename it so handlers are easy to grep and distinguish from plain helpers.",
                "matcher": {
                    "type": "symbol-scan",
                    "file_pattern": "(?i)routes?/.*\\.tsx?$",
                    "kind": "function",
                    "exported": true,
                    "name_pattern": "^handle[A-Z]",
                    "negate": true
                }
            }
        ]
    }"#;

    fn http_conventions_pack() -> RulePackDef {
        serde_json::from_str(HTTP_CONVENTIONS_JSON).expect("parse inlined http-conventions fixture")
    }

    #[test]
    fn http_conventions_flags_unversioned_provided_endpoint() {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: "routes/authRoutes.ts".into(),
            text: String::new(),
            symbols: vec![],
            io: Some(IoFacts {
                provides: vec![io_provide("http", "GET /authen/getUserInfo", 12)],
                consumes: vec![],
            }),
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let f = eval_pack(&http_conventions_pack(), &ctx);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].rule_id, "http-conventions/endpoint-version-prefix");
        assert_eq!(f[0].line, 12);
    }

    #[test]
    fn http_conventions_does_not_flag_versioned_endpoint() {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: "routes/authRoutes.ts".into(),
            text: String::new(),
            symbols: vec![],
            io: Some(IoFacts {
                provides: vec![io_provide("http", "GET /api/v1/authen/getUserInfo", 12)],
                consumes: vec![],
            }),
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let f = eval_pack(&http_conventions_pack(), &ctx);
        assert!(f
            .iter()
            .all(|x| x.rule_id != "http-conventions/endpoint-version-prefix"));
    }

    #[test]
    fn http_conventions_flags_unversioned_fetch_and_unresolved_dynamic_fetch() {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: "src/api/client.ts".into(),
            text: String::new(),
            symbols: vec![],
            io: Some(IoFacts {
                provides: vec![],
                consumes: vec![
                    io_consume("http", Some("GET /authen/getUserInfo"), 7),
                    io_consume("http", None, 15),
                    io_consume("http", Some("GET /api/v2/orders"), 22),
                ],
            }),
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let f = eval_pack(&http_conventions_pack(), &ctx);
        let hits: Vec<_> = f
            .iter()
            .filter(|x| x.rule_id == "http-conventions/unversioned-fetch")
            .collect();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|x| x.line == 7));
        assert!(hits.iter().any(|x| x.line == 15));
    }

    #[test]
    fn http_conventions_flags_exported_handler_with_bad_naming() {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: "routes/authRoutes.ts".into(),
            text: String::new(),
            symbols: vec![
                symbol("handleLogin", SourceSymbolKind::Function, 3, true),
                symbol("login", SourceSymbolKind::Function, 9, true),
            ],
            io: None,
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let f = eval_pack(&http_conventions_pack(), &ctx);
        let hits: Vec<_> = f
            .iter()
            .filter(|x| x.rule_id == "http-conventions/exported-handler-naming")
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 9);
    }

    // --- RegexSet multi-pattern pre-filter (pure optimization — must be observationally identical) ---

    fn findings_as_json(f: &[Finding]) -> Vec<serde_json::Value> {
        f.iter().map(|x| serde_json::to_value(x).unwrap()).collect()
    }

    #[test]
    fn prefilter_matches_unoptimized_findings_across_java_security_pack() {
        let files = vec![
            SourceFile {
                loop_spans: Vec::new(),
                rel: "C.java".into(),
                text: r#"Query q = em.createQuery("SELECT u FROM User u WHERE u.login = '" + login + "'");"#
                    .into(),
                symbols: vec![],
                io: None,
            },
            SourceFile {
                loop_spans: Vec::new(),
                rel: "D.java".into(),
                text: "MessageDigest md = MessageDigest.getInstance(\"MD5\");\nCipher.getInstance(\"DES/CBC/PKCS5Padding\");\n// legacy DigestUtils.md5DigestAsHex\n".into(),
                symbols: vec![],
                io: None,
            },
            SourceFile {
                loop_spans: Vec::new(),
                rel: "E.java".into(),
                text: "public class E { void noop() { System.out.println(\"nothing interesting\"); } }".into(),
                symbols: vec![],
                io: None,
            },
            SourceFile {
                loop_spans: Vec::new(),
                rel: "F.java".into(),
                text: "public class F {\n  void run() {\n    String[] cmd = { \"sh\", \"-c\", \"ping \" + host };\n    Runtime.getRuntime().exec(cmd);\n  }\n}".into(),
                symbols: vec![method("run", 2, 5)],
                io: None,
            },
        ];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let pack = pack();
        let optimized = eval_pack(&pack, &ctx);
        let unoptimized = eval_pack_no_prefilter(&pack, &ctx);
        assert!(!optimized.is_empty());
        assert_eq!(findings_as_json(&optimized), findings_as_json(&unoptimized));
    }

    #[test]
    fn prefilter_respects_require_file_cheap_skip_semantics_unchanged() {
        let pack: RulePackDef = serde_json::from_str(
            r#"{
                "id": "t",
                "framework": "any",
                "rules": [
                    {
                        "id": "r1",
                        "severity": "info",
                        "message": "m",
                        "matcher": {
                            "type": "line-scan",
                            "file_pattern": ".*",
                            "require_file": "NEEDLE",
                            "line_pattern": "foo"
                        }
                    }
                ]
            }"#,
        )
        .unwrap();
        let files = vec![
            // RegexSet candidate (contains "foo") but require_file ("NEEDLE") is absent -> must stay skipped.
            SourceFile {
                loop_spans: Vec::new(),
                rel: "a.txt".into(),
                text: "foo bar".into(),
                symbols: vec![],
                io: None,
            },
            // RegexSet candidate AND require_file present -> must be flagged.
            SourceFile {
                loop_spans: Vec::new(),
                rel: "b.txt".into(),
                text: "foo NEEDLE".into(),
                symbols: vec![],
                io: None,
            },
        ];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let optimized = eval_pack(&pack, &ctx);
        let unoptimized = eval_pack_no_prefilter(&pack, &ctx);
        assert_eq!(findings_as_json(&optimized), findings_as_json(&unoptimized));
        assert_eq!(optimized.len(), 1);
        assert_eq!(optimized[0].file, "b.txt");
    }

    // --- eval_pack_profiled (rule profiling substrate) ---

    #[test]
    fn eval_pack_profiled_findings_match_eval_pack_exactly() {
        let files = vec![
            SourceFile {
                loop_spans: Vec::new(),
                rel: "C.java".into(),
                text: r#"Query q = em.createQuery("SELECT u FROM User u WHERE u.login = '" + login + "'");"#
                    .into(),
                symbols: vec![],
                io: None,
            },
            SourceFile {
                loop_spans: Vec::new(),
                rel: "D.java".into(),
                text: "MessageDigest.getInstance(\"MD5\");\n".into(),
                symbols: vec![],
                io: None,
            },
        ];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let pack = pack();
        let plain = eval_pack(&pack, &ctx);
        let (profiled, timings) = eval_pack_profiled(&pack, &ctx);
        assert_eq!(findings_as_json(&plain), findings_as_json(&profiled));
        assert!(!plain.is_empty());

        assert_eq!(timings.len(), pack.rules.len());
        let ids: std::collections::HashSet<&str> =
            timings.iter().map(|t| t.rule_id.as_str()).collect();
        assert_eq!(ids.len(), timings.len(), "duplicate rule_id in timings");
        for t in &timings {
            assert!(t.rule_id.starts_with("java-security/"));
        }
        let total_findings: usize = timings.iter().map(|t| t.findings).sum();
        assert_eq!(total_findings, plain.len());
    }

    #[test]
    fn eval_pack_profiled_on_empty_pack_yields_no_timings() {
        let pack = RulePackDef {
            id: "empty".into(),
            framework: "any".into(),
            schema_version: 1,
            rules: vec![],
        };
        let files = vec![];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        let (findings, timings) = eval_pack_profiled(&pack, &ctx);
        assert!(findings.is_empty());
        assert!(timings.is_empty());
    }

    // --- DSL v2 extensions (rule-pack-porting.md, extensions #1-4) ---

    /// Builds a one-rule pack from a full inline rule JSON object — needed here since `suppress_marker`
    /// lives on `RuleDef`, not inside `matcher`.
    fn rule_pack(rule_json: &str) -> RulePackDef {
        let src = format!(r#"{{"id":"t","framework":"any","rules":[{rule_json}]}}"#);
        serde_json::from_str(&src).expect("parse inline rule pack")
    }

    fn scan_pack(
        pack: &RulePackDef,
        rel: &str,
        src: &str,
        symbols: Vec<SourceSymbol>,
    ) -> Vec<Finding> {
        let files = vec![SourceFile {
            loop_spans: Vec::new(),
            rel: rel.into(),
            text: src.into(),
            symbols,
            io: None,
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        eval_pack(pack, &ctx)
    }

    // --- extension #1: method-scan `absent` labels ---

    fn toctou_like_pack() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfindOne\\(","label":"read"},{"pattern":"\\bcreate\\(","label":"write"}],"trigger":"write","absent":[{"pattern":"\\btry\\s*\\{","label":"guard"}]}}"#,
        )
    }

    #[test]
    fn absent_label_does_not_veto_when_no_guard_present() {
        let src = "async function f() {\n  const x = await t.findOne();\n  if (!x) {\n    await t.create();\n  }\n}\n";
        let f = scan_pack(&toctou_like_pack(), "f.ts", src, vec![method("f", 1, 6)]);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn absent_label_vetoes_finding_when_guard_present_in_same_span() {
        let src = "async function f() {\n  const x = await t.findOne();\n  if (!x) {\n    try {\n      await t.create();\n    } catch (e) {}\n  }\n}\n";
        let f = scan_pack(&toctou_like_pack(), "f.ts", src, vec![method("f", 1, 8)]);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn absent_label_only_vetoes_within_the_same_span_not_a_sibling_symbol() {
        // The guard lives in a different function body, so this must still fire.
        let src = "async function f() {\n  const x = await t.findOne();\n  await t.create();\n}\nfunction g() {\n  try {\n  } catch (e) {}\n}\n";
        let f = scan_pack(
            &toctou_like_pack(),
            "f.ts",
            src,
            vec![method("f", 1, 3), method("g", 5, 7)],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    // --- extension #2: line-scan `exclude_pattern` ---

    fn as_cast_pack() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bas\\b","exclude_pattern":"^\\s*import\\b"}}"#,
        )
    }

    #[test]
    fn exclude_pattern_still_flags_a_plain_as_cast() {
        let f = scan_pack(&as_cast_pack(), "f.ts", "const x = y as Foo;\n", vec![]);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn exclude_pattern_skips_import_alias_as() {
        let f = scan_pack(
            &as_cast_pack(),
            "f.ts",
            "import { useState as useLocalState } from \"react\";\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn exclude_pattern_only_skips_matching_lines_not_the_whole_file() {
        let f = scan_pack(
            &as_cast_pack(),
            "f.ts",
            "import { useState as useLocalState } from \"react\";\nconst x = y as Foo;\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 2);
    }

    // --- extension #3: inline ok-marker suppression ---

    fn marker_line_pack() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"info","message":"m","suppress_marker":"as-ok","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bas\\b"}}"#,
        )
    }

    #[test]
    fn suppress_marker_on_the_same_line_suppresses_line_scan_finding() {
        let f = scan_pack(
            &marker_line_pack(),
            "f.ts",
            "const x = y as Foo; // as-ok: guaranteed by caller\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn suppress_marker_on_the_line_above_suppresses_line_scan_finding() {
        let f = scan_pack(
            &marker_line_pack(),
            "f.ts",
            "// as-ok: guaranteed by caller\nconst x = y as Foo;\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn suppress_marker_two_lines_above_does_not_suppress() {
        let f = scan_pack(
            &marker_line_pack(),
            "f.ts",
            "// as-ok: guaranteed by caller\nfunction f() {\n  return (\n    y as Foo);\n}\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn suppress_marker_four_lines_above_does_not_suppress() {
        let f = scan_pack(
            &marker_line_pack(),
            "f.ts",
            "// as-ok: too far\nfunction f() {\n  const a = 1;\n  return (\n    y as Foo);\n}\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn suppress_marker_does_not_reach_a_sibling_finding_two_lines_below_it() {
        let f = scan_pack(
            &marker_line_pack(),
            "f.ts",
            "// as-ok: vetted for the next line only\nconst a = x as Foo;\nconst b = y as Bar;\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 3);
    }

    #[test]
    fn unrelated_marker_text_does_not_suppress() {
        let f = scan_pack(
            &marker_line_pack(),
            "f.ts",
            "const x = y as Foo; // unrelated-ok\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn no_marker_at_all_does_not_suppress() {
        let f = scan_pack(&marker_line_pack(), "f.ts", "const x = y as Foo;\n", vec![]);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    // --- `--`-comment marker recognition, gated to `.sql` files (destructive-migration ergonomics) ---

    fn marker_line_pack_sql() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"info","message":"m","suppress_marker":"as-ok","matcher":{"type":"line-scan","file_pattern":"\\.sql$","line_pattern":"\\bas\\b"}}"#,
        )
    }

    #[test]
    fn dash_dash_marker_on_the_same_line_suppresses_line_scan_finding_in_a_sql_file() {
        let f = scan_pack(
            &marker_line_pack_sql(),
            "f.sql",
            "SELECT id as x; -- as-ok: guaranteed by caller\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn dash_dash_marker_on_the_line_above_suppresses_line_scan_finding_in_a_sql_file() {
        let f = scan_pack(
            &marker_line_pack_sql(),
            "f.sql",
            "-- as-ok: guaranteed by caller\nSELECT id as x;\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn dash_dash_marker_is_not_recognized_outside_a_sql_file() {
        // Same rule, same marker text, but a `.ts` file — `--` is not a comment there (`--x` is a
        // decrement), so the `--`-marker recognizer must never activate for it.
        let pack = rule_pack(
            r#"{"id":"r","severity":"info","message":"m","suppress_marker":"as-ok","matcher":{"type":"line-scan","file_pattern":"\\.(ts|sql)$","line_pattern":"\\bas\\b"}}"#,
        );
        let f = scan_pack(
            &pack,
            "f.ts",
            "const x = y as Foo; -- as-ok: nope\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn slash_slash_marker_still_suppresses_in_a_sql_file() {
        // The `--` recognizer is additive: a `.sql` file's `//`-form marker (unusual, but not forbidden)
        // still suppresses too.
        let f = scan_pack(
            &marker_line_pack_sql(),
            "f.sql",
            "SELECT id as x; // as-ok: guaranteed by caller\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn dash_dash_unrelated_marker_text_does_not_suppress_in_a_sql_file() {
        let f = scan_pack(
            &marker_line_pack_sql(),
            "f.sql",
            "SELECT id as x; -- unrelated-ok\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    fn marker_method_pack_sql() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","suppress_marker":"n+1-ok","matcher":{"type":"method-scan","file_pattern":"\\.sql$","patterns":[{"pattern":"\\bfor\\s*\\(","label":"loop"},{"pattern":"\\bfindOne\\(","label":"call"}],"trigger":"call"}}"#,
        )
    }

    #[test]
    fn dash_dash_marker_suppresses_method_scan_finding_in_a_sql_file() {
        let src = "async function f(ids) {\n  for (const id of ids) {\n    -- n+1-ok: batched elsewhere\n    await t.findOne(id);\n  }\n}\n";
        let f = scan_pack(
            &marker_method_pack_sql(),
            "f.sql",
            src,
            vec![method("f", 1, 5)],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn dash_dash_marker_absent_leaves_method_scan_finding_intact_in_a_sql_file() {
        let src = "async function f(ids) {\n  for (const id of ids) {\n    await t.findOne(id);\n  }\n}\n";
        let f = scan_pack(
            &marker_method_pack_sql(),
            "f.sql",
            src,
            vec![method("f", 1, 4)],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    fn marker_method_pack() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","suppress_marker":"n+1-ok","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfor\\s*\\(","label":"loop"},{"pattern":"\\bfindOne\\(","label":"call"}],"trigger":"call"}}"#,
        )
    }

    #[test]
    fn suppress_marker_with_regex_metacharacters_suppresses_method_scan_finding() {
        let src = "async function f(ids) {\n  for (const id of ids) {\n    // n+1-ok: batched elsewhere\n    await t.findOne(id);\n  }\n}\n";
        let f = scan_pack(&marker_method_pack(), "f.ts", src, vec![method("f", 1, 5)]);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn suppress_marker_absent_leaves_method_scan_finding_intact() {
        let src = "async function f(ids) {\n  for (const id of ids) {\n    await t.findOne(id);\n  }\n}\n";
        let f = scan_pack(&marker_method_pack(), "f.ts", src, vec![method("f", 1, 4)]);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    // --- extension #4: method-scan innermost-span priority ---

    fn call_scan_pack() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfoo\\(","label":"call"}],"trigger":"call"}}"#,
        )
    }

    #[test]
    fn overlapping_class_and_method_spans_are_evaluated_only_at_the_innermost_span() {
        // Mirrors the real TS parser: a class symbol's span covers the whole class, and each method also
        // gets its own nested sub-symbol span. Without extension #4 this would double-count.
        let src = "class C {\n  method() {\n    foo();\n  }\n}\n";
        let outer = SourceSymbol {
            id: "f.ts#C".into(),
            file: "f.ts".into(),
            name: "C".into(),
            kind: SourceSymbolKind::Class,
            line: 1,
            exported: false,
            is_default: false,
            body_start: Some(1),
            body_end: Some(5),
            write_sites: Vec::new(),
        };
        let inner = method("C.method", 2, 4);
        let f = scan_pack(&call_scan_pack(), "f.ts", src, vec![outer, inner]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 3);
        assert_eq!(f[0].data.as_ref().unwrap()["method"], "C.method");
    }

    #[test]
    fn non_overlapping_sibling_spans_are_each_still_evaluated() {
        let src = "function a() {\n  foo();\n}\nfunction b() {\n  foo();\n}\n";
        let f = scan_pack(
            &call_scan_pack(),
            "f.ts",
            src,
            vec![method("a", 1, 3), method("b", 4, 6)],
        );
        assert_eq!(f.len(), 2, "{f:?}");
    }

    // --- DSL v3 extension: `file_exclude_pattern` (line-scan and method-scan) ---

    fn exclude_pack_line_scan() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","file_exclude_pattern":"(^|/)scripts/","line_pattern":"\\bfoo\\("}}"#,
        )
    }

    #[test]
    fn file_exclude_pattern_skips_a_matching_file_entirely_for_line_scan() {
        let f = scan_pack(
            &exclude_pack_line_scan(),
            "scripts/build.ts",
            "foo();\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn file_exclude_pattern_still_flags_a_non_matching_file_for_line_scan() {
        let f = scan_pack(&exclude_pack_line_scan(), "src/a.ts", "foo();\n", vec![]);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn file_exclude_pattern_absent_does_not_change_line_scan_behavior() {
        let pack = rule_pack(
            r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bfoo\\("}}"#,
        );
        let f = scan_pack(&pack, "scripts/build.ts", "foo();\n", vec![]);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn file_exclude_pattern_bad_regex_skips_the_whole_line_scan_rule() {
        let pack = rule_pack(
            r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","file_exclude_pattern":"(","line_pattern":"\\bfoo\\("}}"#,
        );
        let f = scan_pack(&pack, "src/a.ts", "foo();\n", vec![]);
        assert!(f.is_empty(), "{f:?}");
    }

    fn exclude_pack_method_scan() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","file_exclude_pattern":"(^|/)scripts/","patterns":[{"pattern":"\\bfoo\\(","label":"call"}],"trigger":"call"}}"#,
        )
    }

    #[test]
    fn file_exclude_pattern_skips_a_matching_file_entirely_for_method_scan() {
        let src = "function a() {\n  foo();\n}\n";
        let f = scan_pack(
            &exclude_pack_method_scan(),
            "scripts/build.ts",
            src,
            vec![method("a", 1, 3)],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn file_exclude_pattern_still_flags_a_non_matching_file_for_method_scan() {
        let src = "function a() {\n  foo();\n}\n";
        let f = scan_pack(
            &exclude_pack_method_scan(),
            "src/a.ts",
            src,
            vec![method("a", 1, 3)],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    // --- DSL v4 extension: `require_file_absent` (line-scan) ---

    fn require_file_absent_pack() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bsetInterval\\s*\\(","require_file_absent":["\\bclearInterval\\s*\\("]}}"#,
        )
    }

    #[test]
    fn require_file_absent_fires_when_the_absent_pattern_is_missing_from_the_file() {
        let f = scan_pack(
            &require_file_absent_pack(),
            "f.ts",
            "const id = setInterval(tick, 1000);\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 1);
    }

    #[test]
    fn require_file_absent_skips_the_file_when_the_absent_pattern_is_present_anywhere() {
        let f = scan_pack(
            &require_file_absent_pack(),
            "f.ts",
            "const id = setInterval(tick, 1000);\nfunction teardown() {\n  clearInterval(id);\n}\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn require_file_absent_empty_list_is_a_no_op() {
        let pack = rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bsetInterval\\s*\\("}}"#,
        );
        let f = scan_pack(
            &pack,
            "f.ts",
            "const id = setInterval(tick, 1000);\nclearInterval(id);\n",
            vec![],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn require_file_absent_bad_regex_skips_the_whole_rule() {
        let pack = rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bsetInterval\\s*\\(","require_file_absent":["("]}}"#,
        );
        let f = scan_pack(
            &pack,
            "f.ts",
            "const id = setInterval(tick, 1000);\n",
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    // --- MethodScan `require_file_absent` (mirrors LineScan's, see field doc) ---

    fn method_scan_require_file_absent_pack() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","require_file_absent":["process\\.on\\s*\\(\\s*['\"]SIG"],"patterns":[{"pattern":"process\\.exit\\s*\\(","label":"exit"}],"trigger":"exit"}}"#,
        )
    }

    #[test]
    fn method_scan_require_file_absent_fires_when_the_absent_pattern_is_missing() {
        let src = "export function shutdown() {\n  process.exit(1);\n}\n";
        let f = scan_pack(
            &method_scan_require_file_absent_pack(),
            "f.ts",
            src,
            vec![method("shutdown", 1, 3)],
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn method_scan_require_file_absent_skips_the_file_when_the_absent_pattern_is_present() {
        let src = "process.on('SIGTERM', () => {\n  process.exit(0);\n});\n";
        let f = scan_pack(
            &method_scan_require_file_absent_pack(),
            "f.ts",
            src,
            vec![method("handler", 1, 3)],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    // --- MethodScan `trigger_in_loop` (structural containment gate, see field doc) ---

    /// Like `scan_pack`, but also lets a test hand-supply `SourceFile::loop_spans` — needed only for the
    /// `trigger_in_loop` tests below, every other `scan_pack` caller has no use for a non-empty vec.
    fn scan_pack_loops(
        pack: &RulePackDef,
        rel: &str,
        src: &str,
        symbols: Vec<SourceSymbol>,
        loop_spans: Vec<(u32, u32)>,
    ) -> Vec<Finding> {
        let files = vec![SourceFile {
            loop_spans,
            rel: rel.into(),
            text: src.into(),
            symbols,
            io: None,
        }];
        let ctx = RuleContext {
            files: &files,
            ir: None,
        };
        eval_pack(pack, &ctx)
    }

    fn trigger_in_loop_pack() -> RulePackDef {
        rule_pack(
            r#"{"id":"r","severity":"warning","message":"Network call issued inside a loop","suppress_marker":"fetch-ok","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfetch\\s*\\(","label":"network"}],"trigger":"network","trigger_in_loop":true}}"#,
        )
    }

    #[test]
    fn trigger_in_loop_fires_for_a_trigger_line_inside_a_loop_span() {
        let src = "function f(ids) {\n  for (const id of ids) {\n    fetch(url(id));\n  }\n}\n";
        let f = scan_pack_loops(
            &trigger_in_loop_pack(),
            "f.ts",
            src,
            vec![method("f", 1, 5)],
            vec![(2, 4)], // the for-loop's own span, header line included per `loop_spans` doc
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 3);
    }

    #[test]
    fn trigger_in_loop_ignores_a_trigger_outside_the_loop_span_even_with_a_sibling_loop_span_in_the_same_body(
    ) {
        // Mono-hub REDDIT shape: a one-shot `fetch` sits earlier in the body, and a `.map` callback span
        // exists elsewhere in the same body but never itself contains a `fetch`. Plain co-occurrence (the
        // pre-`trigger_in_loop` approximation) would have fired on this; the containment gate must not.
        let src = "async function f(items) {\n  const data = fetch(url);\n  const a = 1;\n  const b = 2;\n  const result = items.map(function (item) {\n    return item.id;\n  });\n  return { data, result };\n}\n";
        let f = scan_pack_loops(
            &trigger_in_loop_pack(),
            "f.ts",
            src,
            vec![method("f", 1, 9)],
            vec![(5, 7)], // the `.map` callback body span — does not contain line 2's `fetch`
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn trigger_in_loop_with_no_loop_spans_never_fires() {
        // Graceful degrade: a file with no projected loop spans (external parser / lexical fallback) can
        // never satisfy the trigger, mirroring method-scan's skip of files with no symbol spans.
        let src = "function f(ids) {\n  for (const id of ids) {\n    fetch(url(id));\n  }\n}\n";
        let f = scan_pack_loops(
            &trigger_in_loop_pack(),
            "f.ts",
            src,
            vec![method("f", 1, 5)],
            vec![],
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn trigger_in_loop_fires_for_a_single_line_loop_span() {
        let src = "function f(x) {\n  while (x) fetch(url);\n}\n";
        let f = scan_pack_loops(
            &trigger_in_loop_pack(),
            "f.ts",
            src,
            vec![method("f", 1, 3)],
            vec![(2, 2)], // start == end: a loop whose header and body share one line
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 2);
    }

    #[test]
    fn trigger_in_loop_uses_the_second_match_when_the_first_is_outside_the_loop_span() {
        let src = "async function f(ids) {\n  fetch(warmup);\n  for (const id of ids) {\n    fetch(url(id));\n  }\n}\n";
        let f = scan_pack_loops(
            &trigger_in_loop_pack(),
            "f.ts",
            src,
            vec![method("f", 1, 6)],
            vec![(3, 5)],
        );
        assert_eq!(f.len(), 1, "{f:?}");
        // The out-of-loop match on line 2 neither satisfies the trigger nor supplies the finding's line.
        assert_eq!(f[0].line, 4);
    }

    #[test]
    fn trigger_in_loop_absent_defaults_to_false_and_plain_cooccurrence_still_fires() {
        let pack = rule_pack(
            r#"{"id":"r","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfetch\\s*\\(","label":"network"}],"trigger":"network"}}"#,
        );
        let src = "function f() {\n  fetch(url);\n}\n";
        let f = scan_pack_loops(&pack, "f.ts", src, vec![method("f", 1, 3)], vec![]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 2);
    }

    #[test]
    fn trigger_in_loop_suppress_marker_above_the_in_loop_trigger_suppresses() {
        let src = "async function f(ids) {\n  for (const id of ids) {\n    // fetch-ok: batched via queue\n    fetch(url(id));\n  }\n}\n";
        let f = scan_pack_loops(
            &trigger_in_loop_pack(),
            "f.ts",
            src,
            vec![method("f", 1, 6)],
            vec![(2, 5)],
        );
        assert!(f.is_empty(), "{f:?}");
    }
}
