//! Matcher shapes — the `Matcher` enum and its four variant structs (`LineScan`/`MethodScan`/
//! `SymbolScan`/`IoScan`) plus the shared `LabeledPattern`. Split out of `def/mod.rs` (which keeps the
//! pack/rule envelope types and the fragment-expansion logic) purely to stay under the repo's per-file
//! line cap; `def/mod.rs` re-exports every type here so external paths (`zzop_core::dsl::def::Matcher`,
//! `…::LineScan`, …) are unchanged.

use serde::Deserialize;

use crate::{io::IoKind, ir::SourceSymbolKind};

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
    /// Mask the interior of every closed string literal on a line (single/double-quote and backtick pairs)
    /// to spaces BEFORE testing any pattern/exclude regex, so a token that only appears inside a string
    /// literal — a code-generation template like `'process.exit(2)'`, a SQL string, an example in a
    /// docstring — does not false-fire. The ORIGINAL line is still used for the snippet. Opt-in per rule
    /// (default `false` = byte-identical to today): rules whose whole point is matching inside strings
    /// (hardcoded-secret, sql-in-string, private-key-committed) must leave it off. See
    /// `crate::dsl::string_mask::mask_string_literals` for the exact masking (line-local: an unterminated
    /// multi-line string opener is left intact).
    #[serde(default)]
    pub strip_string_literals: bool,
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

pub(super) fn default_snippet_max() -> usize {
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
    /// Mask closed string-literal interiors on each line to spaces before testing any `patterns`/`absent`
    /// regex, so a token inside a string literal (a code-gen template like `'process.exit(2)'`, an example
    /// in a docstring) does not false-fire. The original line is kept for the snippet + `marker_suppresses`.
    /// Opt-in per rule (default `false` = byte-identical to today) — see `LineScan::strip_string_literals`
    /// and `crate::dsl::string_mask::mask_string_literals`.
    #[serde(default)]
    pub strip_string_literals: bool,
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

/// Query over a tree's cross-layer IO (every `IoProvide`/`IoConsume`, projected whole-tree since the 2026
/// projection redesign — see `crate::dsl::eval_pack_io_scan`), for boundary-convention rules
/// line-scan/method-scan can't express (e.g. "every HTTP endpoint must be versioned under
/// `/api/v[0-9]+/`", or "every mutating route must carry an `auth-guarded` attribute"). Filters combine
/// with AND, evaluated cheap-first: `file_exclude_pattern` right after `file_pattern`, then `direction`
/// selects `provides`/`consumes`/`any`, `kind` is an exact match, then `key_pattern`/`negate` (below), then
/// the four additive gates below (`symbol_pattern`, `attr_present`, `attr_absent`,
/// `anchor_exclude_pattern`) — each a plain conjunctive filter evaluated AFTER `negate` has already
/// resolved `key_pattern`'s role; `negate` itself only ever flips `key_pattern`, never these newer fields.
/// `key_pattern` + `negate` work like `SymbolScan`'s. An entry with `key: None` (unresolved) never matches
/// `key_pattern` — under `negate: true` that makes it a hit.
#[derive(Debug, Clone, Deserialize)]
pub struct IoScan {
    /// Target file-path regex — see struct doc for why this field is required.
    pub file_pattern: String,
    /// Optional path regex — an entry whose `file` matches this is skipped entirely, evaluated right
    /// after `file_pattern` (cheapest gate first). Same rationale and shape as `LineScan`'s field of the
    /// same name (e.g. excluding `${test-paths-stories}` so a composed whole-tree provide/consume from a
    /// test/story file never reaches the rule) — fragment-expanded by `RulePackDef::expand_fragments`
    /// exactly like `LineScan::file_exclude_pattern` is.
    #[serde(default)]
    pub file_exclude_pattern: Option<String>,
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
    /// Regex on `IoProvide::symbol` — PROVIDES-ONLY evidence: a consume never carries a symbol, so when
    /// this is set a consume entry never matches, and a provide whose `symbol` is `None` never matches
    /// either (never-guess). Unlike `key_pattern`, `negate` does NOT flip this field's role — it is a
    /// plain "must match" gate evaluated after `negate` has already resolved `key_pattern` (see struct
    /// doc).
    #[serde(default)]
    pub symbol_pattern: Option<String>,
    /// Entry matches only when the tree's `AttributeStore` has NO truthy value for
    /// `route_attr(entry.kind, entry.key, attr_absent)` (see `crate::attributes::AttributeStore::route_attr`
    /// — exact `IoKey` wins, else the longest covering `PathScope`; truthiness via `attr_is_truthy`). An
    /// entry with no resolved key (an unresolved consume) has nothing to look up, so it always satisfies
    /// this gate. A plain string, not a regex — never regex-checked by `pack_regex_issues`.
    #[serde(default)]
    pub attr_absent: Option<String>,
    /// Entry matches only when that same `route_attr` lookup IS truthy. An entry with no resolved key
    /// never satisfies this gate (nothing to look up). A plain string, not a regex — never regex-checked
    /// by `pack_regex_issues`.
    #[serde(default)]
    pub attr_present: Option<String>,
    /// Regex applied to the ANCHOR LINE's own text (the provide/consume's own source line, fetched via
    /// `IoScanTreeContext::anchor_line`). When the callback returns `None` (no source text reachable —
    /// e.g. envelope mode with no native source), the exclusion simply does not apply: lexical carve-outs
    /// are a native-tree convenience, honestly absent without source text, never a guessed match.
    #[serde(default)]
    pub anchor_exclude_pattern: Option<String>,
}
