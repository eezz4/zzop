//! Matcher shapes — the `Matcher` enum with one struct per variant (the enum below is the list; this
//! header does not keep a second copy) plus the shared `LabeledPattern`. Split out of `def/mod.rs` (which keeps the
//! pack/rule envelope types and the fragment-expansion logic) purely to stay under the repo's per-file
//! line cap; `def/mod.rs` re-exports every type here so external paths (`zzop_core::dsl::def::Matcher`,
//! `…::LineScan`, …) are unchanged.
//!
//! `MethodScan` and `CallScan` live one level down ([`method_scan`], [`call_scan`]) for that same
//! line-cap reason and are re-exported here, so nothing outside this file can tell the difference — see
//! those modules' own headers.

use serde::Deserialize;

use crate::{io::IoKind, ir::SourceSymbolKind};

mod call_scan;
mod literal_scan;
mod method_scan;

pub use call_scan::CallScan;
pub use literal_scan::LiteralScan;
pub use method_scan::MethodScan;

/// Matcher — dispatched on the `type` tag. v0 was lexical line-scan + method-scan; symbol-scan and io-scan
/// (below) are the first IR-query matchers. Whole-graph queries (cross-file/cross-layer) still stay native.
///
/// The wire tag is the variant name in kebab-case, so a pack writes `"type": "call-scan"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Matcher {
    LineScan(LineScan),
    MethodScan(MethodScan),
    SymbolScan(SymbolScan),
    IoScan(IoScan),
    /// Query over a file's projected `call_sites` — see [`CallScan`].
    CallScan(CallScan),
    /// Query over a file's projected `string_literals` (name + value hash + value entropy, never the
    /// value) — see [`LiteralScan`].
    LiteralScan(LiteralScan),
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
    /// Structural LINE gate over the projected call-site channel: when set, a line that matched
    /// `line_pattern`/`any` only fires if a `SourceFile::call_sites` entry of exactly this `kind` sits
    /// on that SAME line. The line-scan twin of `MethodScan::require_call_kind`, at line rather than
    /// span granularity — W3's "structure only the exec witness": the `any` arms keep carrying the
    /// lexical co-occurrence evidence (an interpolation shape, a concatenation), and this gate adds the
    /// parser's word that the line really calls the process API (so the same spelling inside a string
    /// literal, a comment, or on a non-platform receiver no longer fires).
    ///
    /// Degrade direction: SILENCE — a file with no projected call sites (degraded parse, lexical
    /// fallback, unresolvable spelling) can never fire a gated rule, where the ungated regex used to.
    /// A rule setting this trades that recall for the string/comment/receiver false-positive class and
    /// must disclose the trade in its message. Kind spellings are bound by `RULE_READ_CALL_KINDS` via
    /// `call_kind_readers.rs`, the same contract `CallScan::kind` is under.
    #[serde(default)]
    pub line_call_kind: Option<String>,
    /// Optional path regex — a file whose `rel` path matches this is skipped entirely. `file_pattern` is
    /// positive-only and `regex` has no lookaround, so this is the escape hatch for "this extension but
    /// NOT under `scripts/`".
    #[serde(default)]
    pub file_exclude_pattern: Option<String>,
    /// DECLARATION gate — the file must carry a truthy `attr_present` attribute
    /// (`AttributeStore::path_attr(file.rel, attr_present)`: an exact `EntityRef::File` target wins, else
    /// the longest covering `EntityRef::PathScope`; truthiness via `attr_is_truthy`). A plain attribute
    /// name, not a regex — never regex-checked by `pack_regex_issues`. The line-scan twin of
    /// `IoScan::attr_present`, differing only in what the attribute is looked up AGAINST: a route key
    /// there, this file's own path here.
    ///
    /// WHERE THIS IS EVALUATED, and why not here. NOT inside `crate::dsl::line_scan::eval_line_scan` —
    /// that runs in the per-file fused pass whose findings are CACHED under
    /// `(content_hash, parser_fingerprint, scope, ruleset_fingerprint)`, a key with no attribute
    /// ingredient. A gate applied there would bake a declaration into a cache entry that outlives it, so
    /// editing `zzop.config.jsonc` would leave stale findings behind with nothing to invalidate them.
    /// Instead the gate is a WHOLE-TREE POST-FILTER (`crate::dsl::apply_attr_gates`) the engine runs at
    /// assemble, after the per-file pass and before `merge_findings`, recomputed every run — exactly the
    /// placement `severity_overrides`/`suppressions` already use for the same reason (see
    /// `zzop_engine::cache`'s module doc, "Ruleset fingerprint composition"). The cache therefore stores
    /// UNGATED findings, which is the honest thing for it to store: they are what the rule produced.
    ///
    /// Consequence a pack author must know: the gate can only ever REMOVE findings, and it is file-level.
    /// A gate that needed to change WHICH lines match, or that varied within a file, could not live in a
    /// post-filter at all.
    #[serde(default)]
    pub attr_present: Option<String>,
    /// Negated mirror of [`Self::attr_present`] — the same `path_attr` lookup, inverted: the file must
    /// carry NO truthy value for this attribute. Same plain-string, same post-filter placement (see
    /// `attr_present`'s doc for both).
    ///
    /// On its own this gate is INERT when nobody declares the key: every file trivially lacks it, so the
    /// rule fires everywhere. That is correct for an engine-MINTED attribute (io-scan's `auth-guarded`:
    /// a tree where nothing is guarded is a tree where every mutating route is genuinely unguarded), and
    /// wrong for a USER-DECLARED one, where it silently turns "flag reads outside the declared zone" back
    /// into "flag every read" — the guess the declaration existed to replace. A rule of the second kind
    /// pairs this with [`Self::require_attr_declared`].
    #[serde(default)]
    pub attr_absent: Option<String>,
    /// PRECONDITION on the declaration channel itself: this rule only runs when the analysis carries at
    /// least one attribute with this key (`AttributeStore::declares` — target shape and truthiness are
    /// irrelevant, an explicit `false` still counts as the producer having spoken). When nothing declares
    /// it, every finding this rule produced is dropped and the silence is DISCLOSED — one warning naming
    /// the rule, the key, how many candidate sites were suppressed, and how to declare it. Silence
    /// without that disclosure would be the exact failure mode this repo treats as cardinal (a run that
    /// reads as clean because a rule never ran).
    ///
    /// Why it is a separate field and not a mode of `attr_absent`: the two answer different questions
    /// ("does this file hold the attribute" vs "does this vocabulary exist at all"), and folding the
    /// second into the first would give one field name two meanings depending on which matcher reads it
    /// — io-scan's `attr_absent` must NOT go silent on an undeclared key (see that field's doc).
    ///
    /// The disclosure is emitted only when at least one finding was actually dropped. A tree with no
    /// candidate sites has nothing to say and says nothing: a key that is present in the output means the
    /// rule ran, and `0` findings there means a real `0` — the honest-channel contract, applied to the
    /// warning channel.
    #[serde(default)]
    pub require_attr_declared: Option<String>,
    /// Max snippet length (truncates long lines).
    #[serde(default = "default_snippet_max")]
    pub snippet_max: usize,
}

/// The value every omitted [`LineScan`] field takes, in ONE place — so a hand-built matcher (test
/// fixture, an embedder constructing a rule in Rust) starts from the same shape a JSON pack with only
/// the required keys deserializes into, and adding a field later touches this impl instead of every
/// literal in the tree. Written out rather than derived because a derived `Default` would give
/// `snippet_max: 0` (silently empty snippets) where serde gives [`default_snippet_max`]; the two must
/// not disagree.
///
/// This is NOT a wire-contract change: serde's required-vs-optional split still comes from the per-field
/// `#[serde(default)]` attributes above (`file_pattern` carries none and stays mandatory in JSON), which
/// is also the axis `crates/facade/src/rule_pack_tests.rs`'s field-parity pin reads.
impl Default for LineScan {
    fn default() -> Self {
        Self {
            file_pattern: String::new(),
            require_file: None,
            require_file_all: Vec::new(),
            require_file_absent: Vec::new(),
            skip_comment_lines: false,
            strip_string_literals: false,
            line_pattern: None,
            any: None,
            exclude_pattern: None,
            line_call_kind: None,
            file_exclude_pattern: None,
            attr_present: None,
            attr_absent: None,
            require_attr_declared: None,
            snippet_max: default_snippet_max(),
        }
    }
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
