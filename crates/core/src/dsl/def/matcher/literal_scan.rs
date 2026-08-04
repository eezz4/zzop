//! [`LiteralScan`] — the matcher over `SourceFile::string_literals`. Lives one level down from
//! `def/matcher.rs` for that file's line cap, exactly like its `call_scan`/`method_scan` siblings;
//! `def/matcher.rs` re-exports it, so `zzop_core::dsl::def::LiteralScan` is the public path.

use serde::Deserialize;

/// Query over a file's projected `string_literals` (`zzop_core::BoundStringLiteral`) — "flag every
/// string literal bound to a name like X whose VALUE carries at least Y bits of entropy", the two
/// judgments a line-scan regex structurally cannot make (name↔value comparison needs a cross-group
/// backreference regex does not have; entropy is not computable from a match at all). The value itself
/// is never available here — only its hash and its extraction-time entropy — so a `LiteralScan` rule
/// CANNOT express a value-text veto (`mock-`/`test-` prefixes on the value); what it gains in exchange
/// is that no candidate secret is ever stored in the cache. Name-side vetoes stay expressible
/// ([`Self::name_exclude_pattern`]).
///
/// Fields are exactly the six FILTERS, nothing presentational: `file_pattern`,
/// `file_exclude_pattern`, `name_pattern`, `name_exclude_pattern`, `entropy_min`,
/// `skip_value_equals_name` — combined with AND, evaluated cheap-first. Five are what today's
/// consuming rule (`security/high-entropy-secret`) reads; `file_exclude_pattern` is the
/// matcher-family-universal file gate every scan matcher carries (same shape and fragment expansion
/// as `LineScan`'s), present for family parity rather than a today-reader. Unlike every sibling scan
/// matcher there is NO `snippet_max` and the finding carries NO source-line snippet — the literal's
/// line IS the candidate secret, so echoing it would launder the no-plaintext contract into the
/// findings cache, stdout and MCP replies; `name` + `line` + `entropy` are the evidence, and the
/// reader opens the line.
///
/// **Degrade direction is SILENCE.** A file whose `string_literals` are empty — no producer for that
/// language, a degraded parse, an oversized file, envelope mode — matches nothing, so a `LiteralScan`
/// rule under-reports rather than over-reports there. Same family as `CallScan`; see
/// `zzop_core::dsl::SourceFile::string_literals`.
#[derive(Debug, Clone, Deserialize)]
pub struct LiteralScan {
    /// Target file-path regex (e.g. `(?i)\.(ts|tsx|js|java|rs|py|go|cs)$`).
    pub file_pattern: String,
    /// Optional path regex — a file whose `rel` matches this is skipped entirely, evaluated right
    /// after `file_pattern`. Same rationale and shape as `LineScan::file_exclude_pattern`, and
    /// fragment-expanded identically.
    #[serde(default)]
    pub file_exclude_pattern: Option<String>,
    /// Regex on the entry's binding NAME, matched against the spelling exactly as written
    /// (`apiKey`, `CLIENT_SECRET`). Absent = every named literal in the selected files.
    #[serde(default)]
    pub name_pattern: Option<String>,
    /// Negated regex on the binding NAME — an entry whose name matches is skipped. This is where a
    /// mock/test/placeholder veto lives on this channel: the VALUE-side veto the line-scan twin uses is
    /// impossible here by design (the value is hashed at extraction — see struct doc), so a rule that
    /// wants both must keep both name-side hygiene here and accept the value-side loss as published.
    #[serde(default)]
    pub name_exclude_pattern: Option<String>,
    /// Entropy floor in TOTAL Shannon bits (see `zzop_core::shannon_entropy_bits` for the exact
    /// formula the producers bake in): the entry only matches when `entropy >= entropy_min`. Absent =
    /// no floor. This is the field that opens `hardcoded-secret`'s measured passphrase blindness — a
    /// threshold is a POLICY VALUE and belongs in the pack (censused by
    /// `scripts/dsl-inline-census.txt`), never hardcoded in this crate.
    #[serde(default)]
    pub entropy_min: Option<f32>,
    /// Veto the sentinel shape `refresh_token = "refresh_token"`: skip an entry whose value is
    /// LITERALLY its own binding name, compared as `value_hash == value_hash_hex(name)` — exact
    /// equality only, which is all a hash can honestly answer (no case-folding, no
    /// separator-normalizing; the line-scan twin's broader shape vetoes do not transfer). `false`
    /// (default) leaves the veto off.
    #[serde(default)]
    pub skip_value_equals_name: bool,
}

/// The value every omitted [`LiteralScan`] field takes, in ONE place — same reason `LineScan`'s
/// hand-written `Default` exists, kept hand-written even though (with no `snippet_max` here) a derived
/// one would currently coincide: a hand-built matcher must start from the shape a JSON pack with only
/// the required keys deserializes into, and that equivalence should be this impl's to state, not an
/// accident of `#[derive]`.
impl Default for LiteralScan {
    fn default() -> Self {
        Self {
            file_pattern: String::new(),
            file_exclude_pattern: None,
            name_pattern: None,
            name_exclude_pattern: None,
            entropy_min: None,
            skip_value_equals_name: false,
        }
    }
}
