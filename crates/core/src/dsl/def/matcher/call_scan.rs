//! [`CallScan`] — the matcher over `SourceFile::call_sites`. Lives one level down from `def/matcher.rs`
//! for that file's line cap, exactly like its `method_scan` sibling; `def/matcher.rs` re-exports it, so
//! `zzop_core::dsl::def::CallScan` is unchanged as a path.

use serde::Deserialize;

use crate::call_sites::CallKind;

use super::default_snippet_max;

/// Query over a file's projected `call_sites` (`zzop_core::call_sites::CallSite`) — "flag every witnessed
/// use of API family X whose callee looks like Y", with the structural gates a text regex cannot express.
///
/// What this buys over `LineScan` on the same idea: the site comes from a PARSE, so a mention inside a
/// string literal or a comment is not one; the same rule covers every language whose parser projects the
/// channel, instead of one regex copy per language; and [`Self::in_loop`] can cross the site against
/// `SourceFile::loop_spans`, which raw text has no way to answer.
///
/// Filters combine with AND, evaluated cheap-first: `file_pattern`, then `file_exclude_pattern` (same
/// string-only cost class), then `kind` (exact match), then `callee_pattern`, then `in_loop`. The three
/// attribute gates are NOT evaluated here — see [`Self::attr_present`].
///
/// **Degrade direction is SILENCE.** A file whose `call_sites` are empty — no producer for that language,
/// a degraded parse, an oversized file — matches nothing, so a `CallScan` rule under-reports rather than
/// over-reports there. Same family as `MethodScan::trigger_in_loop`, and the opposite of
/// `MethodScan::after_in_same_function` (whose absent fact is a no-op). See
/// `zzop_core::dsl::SourceFile::call_sites`.
#[derive(Debug, Clone, Deserialize)]
pub struct CallScan {
    /// Target file-path regex (e.g. `(?i)\.(ts|tsx|js|jsx)$`).
    pub file_pattern: String,
    /// Optional path regex — a file whose `rel` matches this is skipped entirely, evaluated right after
    /// `file_pattern`. Same rationale and shape as `LineScan::file_exclude_pattern`, and
    /// fragment-expanded identically (so `${test-paths-stories}` works here too).
    #[serde(default)]
    pub file_exclude_pattern: Option<String>,
    /// EXACT match against a site's `kind` (e.g. `"console-write"`, `"env-read"` — the spellings
    /// `zzop_core::call_sites`'s constants fix). Absent = every family. A kind naming a family no producer
    /// emits is not an error and not a diagnostic — it is silence, and the pack alone cannot tell the two
    /// apart. What keeps that silence from being invisible is `zzop_core::RULE_READ_CALL_KINDS`, which
    /// records the kinds shipped rules actually name and is bound to them by
    /// `crates/engine/tests/rule_contracts/call_kind_readers.rs`; its second leg additionally rejects a
    /// kind no `CALL_KIND_*` constant spells, which is how a typo here turns something red instead of
    /// matching nothing forever.
    #[serde(default)]
    pub kind: Option<CallKind>,
    /// Regex on the site's `callee`, matched against the spelling EXACTLY as the source wrote it
    /// (`console.error`, `os.environ.get`). This is where a rule's semantic judgment lives — the channel
    /// deliberately carries no `level`/`stream`/`severity` field to judge on, see
    /// `zzop_core::call_sites`'s module doc. Absent = every callee in the selected `kind`.
    #[serde(default)]
    pub callee_pattern: Option<String>,
    /// Regex on the site's [`crate::CallSite::algorithm`], the one argument-derived fact the channel
    /// carries (W4, `hash-call` only today). EVIDENCE-ALLOWING and never-guess on both sides: a site
    /// whose `algorithm` is `None` — the source did not spell one (`createHash(algoVar)`), or the
    /// family carries none — NEVER matches when this field is set, so a rule filtering on it goes
    /// silent rather than approximating. Matched against the spelling exactly as written (`"md5"`,
    /// `"MD5"`, `"SHA-1"`, `"Sha1"`), so a rule's pattern must own its own case-insensitivity —
    /// the same original-spelling contract `callee_pattern` is under.
    #[serde(default)]
    pub algorithm_pattern: Option<String>,
    /// LEXICAL residual on the site's own source line — a regex the line's text must ALSO match for
    /// the site to count. This is how a call-scan rule keeps a co-occurrence half that is genuinely
    /// textual (W4's first use: `weak-password-hash` requires a credential word on the hashing line)
    /// while the trigger itself stays structural — the inverse of `LineScan::line_call_kind`, which
    /// adds a structural gate to a lexical rule.
    ///
    /// Degrade direction: SILENCE, and one step further than the sibling gates — a site whose line the
    /// file text cannot supply (envelope mode carries no source lines) has nothing to match against
    /// and does NOT fire when this field is set. That inverts this module's "anchor text is a courtesy,
    /// not a precondition" note for exactly the rules that opt in: a rule whose CLAIM includes "…on the
    /// same line as X" cannot honestly fire without the line, where a rule whose claim is only about
    /// the site still can. A rule setting this must disclose the trade in its message.
    #[serde(default)]
    pub line_pattern: Option<String>,
    /// STRUCTURAL gate: the site only counts when its line sits inside one of `SourceFile::loop_spans`'
    /// entries — i.e. the parser PROVED the call runs once per iteration. `false` (default) leaves the
    /// gate off entirely.
    ///
    /// Its degrade is silence, twice over: a language projecting `call_sites` but not `loop_spans` (or
    /// vice versa) makes every gated site fail. That is the intended direction — the gate's whole value is
    /// that a finding claims iteration, and a claim with no span behind it would be a guess. Identical
    /// contract to `MethodScan::trigger_in_loop`, reading the same field; `SourceFile::loop_spans`' own
    /// doc owns the eager/lazy boundary that decides what is a span.
    #[serde(default)]
    pub in_loop: bool,
    /// DECLARATION gate — the file must carry a truthy `attr_present` attribute
    /// (`AttributeStore::path_attr`). A plain attribute name, never a regex.
    ///
    /// WHERE THIS IS EVALUATED, and why not here: identical to `LineScan::attr_present`'s contract, for
    /// identical reasons, and that field's doc is the owner — a `CallScan` finding is produced by the same
    /// CACHED per-file pass, whose key `(content_hash, parser_fingerprint, scope, vocabulary_fingerprint,
    /// ruleset_fingerprint)` has no `AttributeStore` ingredient, so a gate applied inside the matcher would
    /// bake a declaration into an entry that outlives it. The gate is therefore a whole-tree POST-FILTER
    /// (`crate::dsl::apply_attr_gates`), recomputed every run. Consequences a pack author must know are the
    /// same two: the gate can only REMOVE findings, and it is file-level.
    #[serde(default)]
    pub attr_present: Option<String>,
    /// Negated mirror of [`Self::attr_present`] — same `path_attr` lookup, inverted, same post-filter
    /// placement. Inert when nobody declares the key (every file trivially lacks it, so the rule fires
    /// everywhere), which is right for an engine-minted attribute and wrong for a user-declared one; a rule
    /// of the second kind pairs this with [`Self::require_attr_declared`]. See `LineScan::attr_absent`.
    #[serde(default)]
    pub attr_absent: Option<String>,
    /// PRECONDITION on the declaration channel itself — the rule only runs when something declares this
    /// attribute key at all (`AttributeStore::declares`), and when nothing does, every finding it produced
    /// is dropped and the silence is DISCLOSED (one warning naming the rule, the key, the suppressed count,
    /// and how to declare it). See `LineScan::require_attr_declared` for the full contract, including why
    /// this is a separate field rather than a mode of `attr_absent`.
    #[serde(default)]
    pub require_attr_declared: Option<String>,
    /// Max snippet length (truncates the site's own source line).
    #[serde(default = "default_snippet_max")]
    pub snippet_max: usize,
}

/// The value every omitted [`CallScan`] field takes, in ONE place — same reason `LineScan`'s hand-written
/// `Default` exists: a derived one would give `snippet_max: 0` (silently empty snippets) where serde gives
/// [`default_snippet_max`], and a hand-built matcher (test fixture, embedder) must start from the shape a
/// JSON pack with only the required keys deserializes into.
impl Default for CallScan {
    fn default() -> Self {
        Self {
            file_pattern: String::new(),
            file_exclude_pattern: None,
            kind: None,
            callee_pattern: None,
            algorithm_pattern: None,
            line_pattern: None,
            in_loop: false,
            attr_present: None,
            attr_absent: None,
            require_attr_declared: None,
            snippet_max: default_snippet_max(),
        }
    }
}
