//! Binds `zzop_core::RULE_READ_CALL_KINDS` to the call kinds this build's SHIPPED RULES actually name.
//!
//! The obligation is `io_kind_readers.rs`'s, verbatim, because the constant's obligation is: that list is
//! what an unread-call-kind disclosure subtracts against, so it decides which kinds a run may call UNREAD.
//! Both drift directions are silent without a machine:
//!
//!   a kind read but unlisted  -> the disclosure names a kind that IS read; it cries wolf, and the next
//!                                reader learns to ignore the one time it is right
//!   a kind listed but unread  -> the disclosure stays quiet about facts nothing acts on, which is the
//!                                exact silence it exists to break
//!
//! # Why this reads packs where its io twin greps source
//!
//! The two vocabularies live in different places, so the honest subject set differs. An io kind is
//! compared by Rust (`kind == "http"`), so `io_kind_readers` greps non-test source and accepts the
//! imprecision of a textual proxy. A call kind is named DECLARATIVELY — `CallScan::kind` in a shipped
//! `rules/dsl/**/*.json` pack — so this test reads the loaded `RulePackDef`s themselves, via the same
//! `load_all_packs()` every sibling contract uses. That is not a proxy: it is the data the engine loads at
//! run time.
//!
//! **The boundary that follows, stated rather than left to be discovered**: a call kind read from RUST
//! (a native analysis comparing `site.kind == "..."`) would be invisible here. None exists today — the
//! channel's declarative consumers are `Matcher::CallScan::kind`, `MethodScan::require_call_kind` and
//! `LineScan::line_call_kind` (all three read below), and no native Rust reader — and
//! `zzop_core::RULE_READ_CALL_KINDS`' own doc carries this note so the constant is not read as broader
//! than its guard. If a native reader ever lands, this test needs io's grep leg added to it, not
//! replaced by it.

use std::collections::BTreeSet;

use zzop_core::Matcher;

/// Every call kind some shipped DSL rule names declaratively — a `CallScan::kind`, a
/// `MethodScan::require_call_kind` (W3's structural co-occurrence gate), or a `LineScan::line_call_kind`
/// (W3's structural line gate). A `CallScan` with `kind: None` contributes nothing on purpose: it selects
/// every family, which is a claim about no particular kind and therefore cannot make one READ in the
/// sense the disclosure means; the two gate fields have no such wildcard form (absent = no gate at all).
///
/// ⚠ SHIPPED means bundled **or exported** ([`crate::load_shipped_packs`]), which is wider than what a
/// default run loads and is the correct population for THIS constant. `RULE_READ_CALL_KINDS`' own doc
/// states its job as *"which call kinds does this BUILD act on"*, and `examples/packs/*.json` is compiled
/// into this binary (`zzop_config::EXAMPLE_PACK_CONTRACTS`) — an exported rule is one `zzop/rules/` copy
/// away from running, not gone. On 2026-08-12 the last `axis: opinion` export moved
/// `console-in-be`/`console-in-loop` (`console-write`) and `env-outside-config` (`env-read`) out of the
/// bundle, which left this census claiming those two kinds were read by nothing. Narrowing the CONSTANT
/// to match would have been the wrong repair in both directions: the eventual unread-call-kind
/// disclosure would then cry wolf at every user who loaded `code-hygiene`, and the parser would still be
/// projecting both families for the rules that read them.
fn kinds_named_by_shipped_rules() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for pack in crate::load_shipped_packs() {
        for rule in &pack.rules {
            match &rule.matcher {
                Matcher::CallScan(m) => {
                    if let Some(kind) = &m.kind {
                        out.insert(kind.clone());
                    }
                }
                Matcher::MethodScan(m) => {
                    if let Some(kind) = &m.require_call_kind {
                        out.insert(kind.clone());
                    }
                }
                Matcher::LineScan(m) => {
                    if let Some(kind) = &m.line_call_kind {
                        out.insert(kind.clone());
                    }
                }
                // Exhaustive on purpose (no `_` catch-all): a NEW matcher kind that grows a
                // call-kind field must add its arm here or fail to compile — a wildcard would let
                // it ship silently outside this census, the hand-list blindness this file exists
                // to prevent.
                Matcher::SymbolScan(_) | Matcher::IoScan(_) | Matcher::LiteralScan(_) => {}
            }
        }
    }
    out
}

/// The set equality, in one assertion with both offender lists named — the same shape
/// `io_kind_readers` splits across two tests, kept as one here because a declarative subject set has no
/// "compared but not joined" middle category to excuse (an io kind can be compared in order to be
/// STRIPPED; a `CallScan::kind` has exactly one meaning, "select this family").
#[test]
fn rule_read_call_kinds_equals_the_kinds_shipped_call_scan_rules_name() {
    let named = kinds_named_by_shipped_rules();
    assert!(
        !named.is_empty(),
        "no shipped rule names a CallScan kind — either every call-scan rule was deleted (then \
         RULE_READ_CALL_KINDS must go back to empty, and this test's subject set with it) or \
         load_all_packs() stopped seeing the packs, which would make this contract vacuously green"
    );
    let declared: BTreeSet<String> = zzop_core::RULE_READ_CALL_KINDS
        .iter()
        .map(|k| (*k).to_string())
        .collect();
    assert_eq!(
        named, declared,
        "zzop_core::RULE_READ_CALL_KINDS disagrees with the kinds shipped `call-scan` rules actually \
         name.\nread by a rule but NOT declared (the unread-call-kind disclosure would cry wolf about a \
         kind something reads — add it to the constant): {:?}\ndeclared but read by NO rule (the \
         disclosure stays silent about facts nothing consumes, the silence it exists to break — remove \
         it from the constant, or land the rule that reads it): {:?}",
        named.difference(&declared).collect::<Vec<_>>(),
        declared.difference(&named).collect::<Vec<_>>(),
    );
}

/// Every kind a shipped rule names must be one the constants in `zzop_core::call_sites` fix a spelling
/// for — the anti-typo leg the set equality above cannot supply on its own (a rule with `kind:
/// "console-writes"` and a constant list containing the same typo would agree with each other perfectly
/// while matching zero sites, because the PRODUCERS spell it the other way).
///
/// The producers are not enumerated here: all six emit `CALL_KIND_*` constants by name
/// (`parser/parser-typescript/src/call_sites.rs` and its `process_exec` submodule, plus each of
/// `parser-python-3`/`parser-go`/`parser-java-21`/`parser-csharp`/`parser-rust`'s
/// `lang/call_sites.rs`), so checking against the constants checks against them.
#[test]
fn every_kind_a_shipped_rule_names_is_a_spelling_some_constant_fixes() {
    const SHIPPED_SPELLINGS: &[&str] = &[
        zzop_core::CALL_KIND_CONSOLE_WRITE,
        zzop_core::CALL_KIND_ENV_READ,
        zzop_core::CALL_KIND_PROCESS_EXEC,
        zzop_core::CALL_KIND_HASH_CALL,
    ];
    let stray: Vec<String> = kinds_named_by_shipped_rules()
        .into_iter()
        .filter(|k| !SHIPPED_SPELLINGS.contains(&k.as_str()))
        .collect();
    assert!(
        stray.is_empty(),
        "shipped `call-scan` rule(s) name call kind(s) no `zzop_core::CALL_KIND_*` constant spells: \
         {stray:?}. The kind vocabulary is open by design (a Mode-B adapter may introduce a family this \
         build never heard of), so this is not a hard error in the loader — but a kind no PRODUCER emits \
         makes its rule silent forever with nothing turning red, which is precisely the failure this \
         contract exists to make loud. Either fix the spelling, or add the constant in the same change \
         as the producer that emits it."
    );
}
