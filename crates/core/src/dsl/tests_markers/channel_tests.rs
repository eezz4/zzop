//! The per-matcher-kind marker judgment ([`super::super::marker_channel`]) and the engine-appended
//! suppress sentence built from it ([`super::super::suppress_hint`]).
//!
//! This judgment used to live in `crates/facade/src/explain/render.rs` alone, as a `match` on
//! `RuleDef::matcher` inside a prose formatter. It moved into `zzop-core` when the engine's
//! finding-construction append became a second consumer: two copies of "which comment leaders can
//! suppress this kind of finding" is the exact defect class the fold that produced these tests exists
//! to remove.

use super::super::{marker_channel, suppress_hint, MarkerChannel};
use super::rule_pack;

/// One rule, spelled as a pack the real loader parses — never a hand-built `RuleDef`, so a matcher
/// shape that stopped deserializing shows up here rather than being papered over by a struct literal.
fn rule(json: &str) -> super::super::RuleDef {
    rule_pack(json).rules.into_iter().next().expect("one rule")
}

fn line_scan() -> super::super::RuleDef {
    rule(
        r#"{"id":"as","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bas\\b"}}"#,
    )
}

fn method_scan() -> super::super::RuleDef {
    rule(
        r#"{"id":"n1","severity":"warning","message":"m","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfor\\s*\\(","label":"loop"},{"pattern":"\\bfindOne\\(","label":"call"}],"trigger":"call"}}"#,
    )
}

fn symbol_scan() -> super::super::RuleDef {
    rule(
        r#"{"id":"sym","severity":"info","message":"m","matcher":{"type":"symbol-scan","file_pattern":"\\.ts$","kind":"function","name_pattern":"^handler$"}}"#,
    )
}

fn io_scan() -> super::super::RuleDef {
    rule(
        r#"{"id":"io","severity":"warning","message":"m","matcher":{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","kind":"http","key_pattern":"^/admin"}}"#,
    )
}

fn call_scan() -> super::super::RuleDef {
    rule(
        r#"{"id":"call","severity":"info","message":"m","matcher":{"type":"call-scan","file_pattern":"\\.ts$","kind":"console-write"}}"#,
    )
}

fn literal_scan() -> super::super::RuleDef {
    rule(
        r#"{"id":"lit","severity":"info","message":"m","matcher":{"type":"literal-scan","file_pattern":"\\.ts$","name_pattern":"token","entropy_min":80}}"#,
    )
}

// --- the judgment itself ---------------------------------------------------------------------

#[test]
fn per_file_matchers_read_the_files_own_leaders() {
    assert_eq!(
        marker_channel(&line_scan().matcher),
        MarkerChannel::PerFileText
    );
    assert_eq!(
        marker_channel(&method_scan().matcher),
        MarkerChannel::PerFileText
    );
}

#[test]
fn symbol_scan_has_no_anchor_line_to_carry_a_marker() {
    assert_eq!(
        marker_channel(&symbol_scan().matcher),
        MarkerChannel::NoAnchorLine
    );
}

/// io-scan is separated from its `//`-or-`#` siblings on ONE axis only: its anchor line is re-read
/// through an engine-supplied callback, which envelope mode answers with `None`. That is the fact the
/// engine's append acts on (it must not tell an envelope user to write a comment), so it is a variant
/// rather than a comment on the multi-language one.
#[test]
fn io_scan_is_the_re_read_channel_and_call_literal_scan_are_not() {
    assert_eq!(
        marker_channel(&io_scan().matcher),
        MarkerChannel::ReReadAnchorLine
    );
    assert_eq!(
        marker_channel(&call_scan().matcher),
        MarkerChannel::MultiLanguageText
    );
    assert_eq!(
        marker_channel(&literal_scan().matcher),
        MarkerChannel::MultiLanguageText
    );
}

// --- the appended sentence -------------------------------------------------------------------

/// THE byte pin for the fold. 106 shipped rules carried this exact sentence in their pack `message`
/// until it moved here; the engine appends it in their place, and a single byte of drift rewrites
/// every one of those messages (and, because the append is pre-cache, invalidates every warm cache).
#[test]
fn the_per_file_sentence_is_byte_identical_to_the_one_the_packs_used_to_carry() {
    assert_eq!(
        suppress_hint(&line_scan()).as_deref(),
        Some("Suppress a vetted case with `// zzop-as-ok`.")
    );
    assert_eq!(
        suppress_hint(&method_scan()).as_deref(),
        Some("Suppress a vetted case with `// zzop-n1-ok`.")
    );
}

/// The multi-language channel discloses the `#` leader, because it really does honor one — this is the
/// information the 33 hand-written variants carried and the 106 identical tails did not.
#[test]
fn the_multi_language_sentence_discloses_the_hash_leader() {
    assert_eq!(
        suppress_hint(&call_scan()).as_deref(),
        Some("Suppress a vetted case with `// zzop-call-ok` (`# zzop-call-ok` in Python).")
    );
}

/// Both silent kinds, for opposite reasons — and neither may be papered over with a sentence telling
/// the reader to write a comment that cannot work.
#[test]
fn no_sentence_is_offered_where_a_comment_could_not_suppress() {
    assert_eq!(
        suppress_hint(&symbol_scan()),
        None,
        "symbol-scan: no anchor line"
    );
    assert_eq!(
        suppress_hint(&io_scan()),
        None,
        "io-scan: anchor line is re-read, and envelope mode has no source to re-read"
    );
}

/// The opt-out that makes the fold byte-safe: an author who already spelled the marker keeps their own
/// wording, and never gets a second sentence bolted on after it.
#[test]
fn a_message_that_already_names_its_marker_gets_no_append() {
    let already = rule(
        r#"{"id":"as","severity":"info","message":"m. Suppress a vetted case (value is provably sanitized upstream) with `// zzop-as-ok`.","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bas\\b"}}"#,
    );
    assert_eq!(suppress_hint(&already), None);
}
