//! `Matcher::LineScan`'s shape — the per-line regex matcher. Split out of the parent `def/matcher.rs`
//! for the same reason that file was split out of `def/mod.rs`: the repo's per-file line cap (adding
//! the one-line-lookback exclusion field pushed the parent over it). The split follows the seam the
//! EVALUATOR side already uses (`dsl/line_scan.rs`), so a reader looking for line-scan finds its shape
//! and its evaluation under the same name — the precedent [`super::method_scan`] set.
//!
//! `def/mod.rs` re-exports `LineScan` unchanged, so `zzop_core::dsl::LineScan` is unaffected.

use serde::Deserialize;

use super::{default_snippet_max, LabeledPattern};

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
    /// One-line-lookback veto: a line matching the main pattern is skipped when the IMMEDIATELY
    /// PRECEDING line matches this regex. Exists for statement CONTINUATIONS a per-line matcher cannot
    /// otherwise see — a formatter-wrapped concise arrow body (`const f = (x) =>` / `  db.create(...)`)
    /// puts the evidence that the promise is returned on the line ABOVE the match, where
    /// `exclude_pattern` never looks (measured FP on `db/unawaited-write`, 2026-08-09). Typically an
    /// end-of-line-anchored continuation shape (e.g. `(?:=>|=)\s*$`), NOT a copy of `exclude_pattern`:
    /// a token like `return` on the previous line usually ends a COMPLETE statement there, and vetoing
    /// on it would silence genuine findings. Exactly one line, never a window, mirroring the marker
    /// lookback (`MARKER_LOOKBACK_LINES` = 1); a continuation starting further up is out of sight, and
    /// a rule leaning on this field should disclose that in its message. Tested against the same
    /// masked text as every other line regex when `strip_string_literals` is set.
    #[serde(default)]
    pub prev_line_exclude_pattern: Option<String>,
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
            prev_line_exclude_pattern: None,
            line_call_kind: None,
            file_exclude_pattern: None,
            attr_present: None,
            attr_absent: None,
            require_attr_declared: None,
            snippet_max: default_snippet_max(),
        }
    }
}
