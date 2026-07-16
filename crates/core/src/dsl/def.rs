//! Rule-pack definition types — the serde surface that deserializes `rules/dsl/*.json`.

use serde::Deserialize;

use crate::{io::IoKind, ir::SourceSymbolKind, Severity};

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
