//! The PER-MATCHER-KIND half of the comment-leader table — where a rule's suppress marker can be
//! written, and whether it can be written at all. The sibling [`super::leaders_for_path`] is the
//! per-file half; both are consulted, and neither answers for the other.
//!
//! Split out of `markers.rs` purely to stay under the repo's per-file line cap
//! (`scripts/check-max-file-lines.sh`); one logical table across the two files.

use crate::dsl::{Matcher, RuleDef};

/// WHERE a matcher kind's suppress marker can be written, and whether it can be written at all.
///
/// One owner, two consumers that would otherwise each spell the same `match rule.matcher`:
/// `zzop-facade`'s `explain` renderer (prose: "in a `//` or `#` line comment ...") and `zzop-engine`'s
/// finding-construction append ([`suppress_hint`], which must not tell a reader to write a comment that
/// cannot work). The variants are the four DISTINCT answers, not the six matcher kinds — kinds that
/// answer identically share one.
///
/// This judgment lived only in `crates/facade/src/explain/render.rs` until the engine's append became
/// its second consumer. Two copies of "which comment leaders can suppress this kind of finding" is the
/// same defect class the fold that created [`suppress_hint`] exists to remove, so it moved down here
/// rather than being copied across.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkerChannel {
    /// `symbol-scan`: the finding names a symbol, not a line, so NO comment anywhere can suppress it.
    /// `RuleDef::suppress_marker` still derives a string; nothing ever consults it.
    NoAnchorLine,
    /// `line-scan` / `method-scan`: the marker is read off the finding's own file text under that
    /// file's own MARKER leaders — [`super::marker_leaders_for_path`], i.e. [`Leaders::Slash`] widened to
    /// [`Leaders::SlashOrSql`] in a `.sql` file and to [`Leaders::SlashOrHash`] in a config file. NOT
    /// [`super::leaders_for_path`], which is the `skip_comment_lines` axis and deliberately excludes `#`.
    PerFileText,
    /// `call-scan` / `literal-scan`: same file text, but a MULTI-LANGUAGE channel — the anchor line can
    /// be Python, so [`Leaders::SlashOrHash`] applies regardless of extension.
    MultiLanguageText,
    /// `io-scan`: [`Leaders::SlashOrHash`] like the multi-language channel, but the anchor line is
    /// re-read through an engine-supplied callback rather than held in `SourceFile::text` — and
    /// envelope mode answers that callback with `None`, because a Normalized-AST envelope carries no
    /// source. Every io-scan marker is therefore INERT under `analyze-envelope`. Split from
    /// [`MarkerChannel::MultiLanguageText`] because that is the difference a consumer acts on, not a
    /// footnote: the engine must not append "add this comment" to a finding whose comment may never be
    /// readable.
    ReReadAnchorLine,
}

// There was a `MarkerChannel::leaders()` here, returning one leader set per channel. It is deleted
// rather than corrected: it had ZERO call sites, and for `PerFileText` it answered `Leaders::Slash`,
// which stopped being the whole truth the moment the marker axis gained the `#` family. A per-CHANNEL
// function cannot answer a per-FILE question, so the first consumer to adopt it would have been handed
// a confident wrong answer. Ask [`super::marker_leaders_for_path`] with the finding's own path instead.

/// [`MarkerChannel`] for a matcher — the ONE place the mapping is written.
pub fn marker_channel(matcher: &Matcher) -> MarkerChannel {
    match matcher {
        Matcher::SymbolScan(_) => MarkerChannel::NoAnchorLine,
        Matcher::IoScan(_) => MarkerChannel::ReReadAnchorLine,
        Matcher::CallScan(_) | Matcher::LiteralScan(_) => MarkerChannel::MultiLanguageText,
        Matcher::LineScan(_) | Matcher::MethodScan(_) => MarkerChannel::PerFileText,
    }
}

/// The suppress-marker sentence `zzop-engine` appends to this rule's findings, or `None` when it must
/// append nothing. 106 shipped rules spelled the `PerFileText` sentence into their own pack `message`
/// by hand — byte-for-byte the same string, carrying no rule-specific information — until it moved
/// here; the bytes below are therefore a PIN, not a wording choice (the append runs before the findings
/// cache, so one changed byte rewrites every message the fold now carries and invalidates every warm
/// cache). That count is the HISTORICAL one and is left in the past tense on purpose: what the byte
/// change would rewrite today is whatever is loaded today, which is not the same set — v0.30.0 moved
/// part of it to `examples/packs/`.
///
/// Three ways to get `None`, and only the third is about the author:
/// - `NoAnchorLine` — no comment can suppress a symbol-scan finding, so offering one would be a lie.
/// - `ReReadAnchorLine` — an io-scan marker is inert in envelope mode; the two shipped io-scan rules
///   spell that limitation out themselves at length, and a future one that does not must still never
///   get a flat "add this comment".
/// - The author already named the marker. That is what makes the fold byte-safe: the rules whose
///   wording says something this sentence cannot (a `#` leader, a carve-out, the envelope caveat) keep
///   their own text and never get a second sentence bolted on after it. How many is not written here —
///   it read `33` until the export and the `.py` marker widening both moved it, and the predicate in
///   [`suppress_hint`]'s body (`rule.message.contains(&marker)`) is the only owner that cannot rot.
///
/// The `PerFileText` sentence names `//` only, and deliberately does not try to name the widenings —
/// `--` is per FILE (`.sql`) and `#` is per file family (`super::marker_leaders_for_path`'s hash
/// family — `leaders_for_path`, the skip axis, has none), and
/// this sentence is per RULE. A rule whose `file_pattern` targets those families should say so in its
/// own `message`, which is exactly the opt-out above: `security/config-file-secret` writes
/// "``Suppress a vetted case with `# zzop-config-file-secret-ok`.``" and keeps it.
pub fn suppress_hint(rule: &RuleDef) -> Option<String> {
    let marker = rule.suppress_marker();
    if rule.message.contains(&marker) {
        return None;
    }
    match marker_channel(&rule.matcher) {
        MarkerChannel::NoAnchorLine | MarkerChannel::ReReadAnchorLine => None,
        MarkerChannel::PerFileText => Some(format!("Suppress a vetted case with `// {marker}`.")),
        MarkerChannel::MultiLanguageText => Some(format!(
            "Suppress a vetted case with `// {marker}` (`# {marker}` in Python)."
        )),
    }
}
