//! THE DRIFT GUARD behind `RuleDef::scan_test_regions` — the opt-out that lets a credential-at-rest rule
//! keep judging lines a parser proved are compiled out of the shipping build (`SourceFile::test_spans`,
//! gated in `zzop_core::dsl::eval`'s `TestRegions`).
//!
//! ## Why a guard and not a list
//! The flag has to be EXPLICIT — nothing derivable distinguishes "this rule judges the commit" from
//! "this rule judges code that runs" (a handful of bundled rules carry no `file_exclude_pattern`, and
//! not every one of them is about credentials — the roster and its size are the `vetoed_files` doc's to
//! recount, and are deliberately not retyped here, having gone stale twice when they were). But an
//! explicit flag maintained by hand is exactly the artifact that
//! rots: the next author adds a credential rule, writes "scans test paths too" in its catalog row, and
//! forgets the flag — and the rule then reports a clean run over a committed key, silently, which is
//! this repo's cardinal failure. Or the reverse: the flag is set and nothing published says so, and a
//! user reading the catalog cannot tell why their test fixtures are being flagged.
//!
//! So the flag's CONSISTENCY is derived rather than listed. Every bundled rule is read out of the packs
//! themselves, its catalog row out of `docs/rules/catalog.md`, its public row out of `site/rules.html`,
//! and the four clauses below bite in both directions. This file never restates which rules are
//! flagged — add one and the guard follows it; add a promise without the flag and it goes red.
//!
//! ## The canonical promise
//! One phrase, [`PROMISE`], case-insensitively. It is already the phrase the shipped artifacts use
//! (`private-key-committed`'s own `message`, and the catalog/site rows for the credential family), so
//! this pins the spelling those surfaces already agreed on rather than inventing a marker.

use std::collections::BTreeMap;

use zzop_core::parse_dsl_pack;
use zzop_core::{Matcher, RuleDef};

/// The published sentence a rule uses to tell users it judges test code. Matched case-insensitively so
/// both the sentence-initial catalog form ("Scans test paths too (…)") and the mid-sentence message form
/// ("so this rule scans test paths too, not just application code") count.
const PROMISE: &str = "scans test paths too";

/// The public rule catalog — the machine-checked SSOT `scripts/check-rules-catalog-sync.sh` already
/// keeps `site/rules.html` in step with on the ID axis. This guard adds the PROSE axis for one phrase,
/// which that script deliberately does not check ("hand-authored prose is intentionally NOT checked").
const CATALOG: &str = include_str!("../../../docs/rules/catalog.md");

/// The published site mirror of the same table. Checked separately rather than trusted to mirror the
/// catalog, because the phrase is exactly the kind of hand-authored prose the sync script exempts.
const SITE: &str = include_str!("../../../site/rules.html");

/// Every bundled rule, by id. Ids are globally unique across the bundled packs (asserted below), which
/// is what lets the catalog and the site key their rows on a bare id.
fn bundled_rules() -> BTreeMap<String, RuleDef> {
    let mut out: BTreeMap<String, RuleDef> = BTreeMap::new();
    for (rel, source) in zzop_config::BUNDLED_PACK_SOURCES {
        let pack =
            parse_dsl_pack(source).unwrap_or_else(|e| panic!("bundled pack {rel} must parse: {e}"));
        for rule in pack.rules {
            let id = rule.id.clone();
            assert!(
                out.insert(id.clone(), rule).is_none(),
                "two bundled packs both declare rule id `{id}` — the catalog and the site key their \
                 rows on a bare id, so this guard (and those two documents) can no longer tell the \
                 two rules apart. Rename one, or teach all three the pack-qualified id."
            );
        }
    }
    assert!(
        !out.is_empty(),
        "read zero bundled rules — the reader is broken, not the packs"
    );
    out
}

/// The one line of `doc` that presents `id`, or a panic naming the gap. Exactly-one is required in both
/// directions: a missing row would let a flagged rule publish nothing, and two rows would make "which
/// one promises?" ambiguous. `check-rules-catalog-sync.sh` already guarantees the row exists; this
/// restates the requirement locally so a failure here reads as a failure here.
fn row<'a>(doc: &'a str, label: &str, id: &str, needle: &str) -> &'a str {
    let rows: Vec<&str> = doc.lines().filter(|l| l.contains(needle)).collect();
    assert_eq!(
        rows.len(),
        1,
        "{label} must present rule `{id}` in exactly one row, found {} — this guard reads the \
         test-region promise off that row",
        rows.len()
    );
    rows[0]
}

fn says_promise(text: &str) -> bool {
    text.to_ascii_lowercase().contains(PROMISE)
}

/// `Some(pattern)` when the rule excludes whole FILE PATHS; `None` for `SymbolScan`, which has no such
/// field at all (and therefore cannot contradict the flag).
fn file_exclude_pattern(rule: &RuleDef) -> Option<&str> {
    match &rule.matcher {
        Matcher::LineScan(m) => m.file_exclude_pattern.as_deref(),
        Matcher::MethodScan(m) => m.file_exclude_pattern.as_deref(),
        Matcher::IoScan(m) => m.file_exclude_pattern.as_deref(),
        Matcher::CallScan(m) => m.file_exclude_pattern.as_deref(),
        Matcher::LiteralScan(m) => m.file_exclude_pattern.as_deref(),
        Matcher::SymbolScan(_) => None,
    }
}

/// CLAUSE A + B, both directions: the flag and what the two public documents promise must agree, per
/// rule. A promise with no flag is a rule whose findings are silently deleted inside `#[cfg(test)]`
/// while its own catalog row says otherwise; a flag with no promise is behavior no reader was told
/// about. Both are reported together so one run names every offender.
#[test]
fn the_scan_test_regions_flag_matches_what_the_catalog_and_the_site_promise() {
    let mut offenders = Vec::new();
    for (id, rule) in bundled_rules() {
        let catalog_row = row(
            CATALOG,
            "docs/rules/catalog.md",
            &id,
            &format!("| `{id}` |"),
        );
        let site_row = row(
            SITE,
            "site/rules.html",
            &id,
            &format!("<code>{id}</code></td>"),
        );
        for (label, promises) in [
            ("docs/rules/catalog.md", says_promise(catalog_row)),
            ("site/rules.html", says_promise(site_row)),
        ] {
            if promises != rule.scan_test_regions {
                offenders.push(if promises {
                    format!(
                        "{id}: {label} promises \"{PROMISE}\" but the rule does NOT set \
                         `scan_test_regions` — every finding it makes inside a proven test region is \
                         dropped, so the published promise is false"
                    )
                } else {
                    format!(
                        "{id}: the rule sets `scan_test_regions` but {label} does not say \
                         \"{PROMISE}\" — it judges test code and no reader was told"
                    )
                });
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`scan_test_regions` has drifted from what the shipped documents promise:\n  {}",
        offenders.join("\n  ")
    );
}

/// CLAUSE C, one direction: a rule's own `message` is shipped to users through `zzop explain` and every
/// finding it produces, so a promise THERE binds just as hard. The converse is deliberately not
/// required — a flagged rule may leave the sentence to its catalog row rather than repeating it in a
/// message that is already the longest field in the pack.
#[test]
fn a_rule_message_that_promises_to_scan_test_paths_must_carry_the_flag() {
    let offenders: Vec<String> = bundled_rules()
        .iter()
        .filter(|(_, rule)| says_promise(&rule.message) && !rule.scan_test_regions)
        .map(|(id, _)| {
            format!("{id}: its own `message` promises \"{PROMISE}\" and the rule is still gated")
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "a rule's shipped message promises what the evaluator does not do:\n  {}",
        offenders.join("\n  ")
    );
}

/// CLAUSE D, one direction: the two exclusions are the same decision reached through two kinds of
/// evidence (a PATH regex, and a parser-proven SPAN), so wanting one and not the other is incoherent —
/// the rule would skip `foo.test.ts` wholesale and then insist on judging `#[cfg(test)]` in the file
/// next to it. Catching that here is what stops the flag from being pasted onto a rule that merely
/// looked adjacent.
///
/// ## What the subject is, and why it stopped being "has a `file_exclude_pattern` at all" (2026-08-26)
/// This clause read the FIELD'S PRESENCE for its whole life, which was sound only because the field was
/// a synonym for the test decision: every bundled use of it was a `${test-paths…}` ref, so "excludes
/// paths" and "excludes TEST paths" named the same set. `security/config-file-secret` broke the synonymy
/// by taking a `file_exclude_pattern` that declines TRANSLATION CATALOGUES (`locales/<lang>/…`,
/// `i18n/<lang>.json` — 87 measured false positives, all of them UI labels, weighted onto languages that
/// do not put spaces between words). Nothing about that pattern says anything about test code, and this
/// clause's own reasoning above — "the rule would skip `foo.test.ts` wholesale" — does not describe it.
/// Reading presence would have forced the rule to give up `scan_test_regions`, which is the credential
/// family's whole point: a key committed inside a fixture is still committed.
///
/// So the subject is now the QUESTION the clause is named after, asked directly:
/// `zzop_core::dsl::declines_shared_test_paths`, which is true when the pattern carries a shared
/// `test-paths*` body (as the whole value, the shape every bundled exclusion has, or as one alternative
/// of a composite). The residual is that predicate's, restated in one line so a reader here need not
/// chase it: a HAND-ROLLED test-path exclusion reads as `false` and would slip through — no bundled rule
/// has one, and `tests_fragments::name_census` is the triage moment where a new spelling gets looked at.
#[test]
fn a_rule_that_scans_test_regions_must_not_also_exclude_test_paths() {
    let rules = bundled_rules();
    let offenders: Vec<String> = rules
        .iter()
        .filter(|(_, rule)| rule.scan_test_regions)
        .filter_map(|(id, rule)| {
            file_exclude_pattern(rule)
                .filter(|p| zzop_core::dsl::declines_shared_test_paths(p))
                .map(|p| {
                    format!("{id}: `scan_test_regions` is set, but `file_exclude_pattern` is {p:?}")
                })
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "these rules decline SPAN-based test exclusion while still declining test PATHS — one of the \
         two is wrong:\n  {}",
        offenders.join("\n  ")
    );
}

/// CLAUSE D's canary, both directions. Narrowing D's subject from "has a `file_exclude_pattern`" to
/// "declines TEST paths" put a predicate between the guard and its population, and a predicate that
/// answered `false` to everything would make D vacuously green — the exact failure the census clause
/// below exists to catch one layer up. So: some bundled exclusion must read as a test-path exclusion
/// (the needle works), and no bundled exclusion may read as one WITHOUT carrying a shared `test-paths*`
/// body verbatim (nothing else is quietly being counted). Neither half names a rule or a count.
#[test]
fn the_test_path_exclusion_predicate_separates_the_two_kinds_of_path_veto() {
    let rules = bundled_rules();
    let excludes: Vec<&str> = rules.values().filter_map(file_exclude_pattern).collect();
    assert!(
        !excludes.is_empty(),
        "no bundled rule carries a `file_exclude_pattern` at all — CLAUSE D has no population and is \
         green over the empty set"
    );
    let declining = excludes
        .iter()
        .filter(|p| zzop_core::dsl::declines_shared_test_paths(p))
        .count();
    assert!(
        declining > 0,
        "not one of the {} bundled `file_exclude_pattern`s reads as a test-path exclusion — the \
         predicate CLAUSE D now filters on is answering `false` to everything, which makes that clause \
         green no matter what the packs say",
        excludes.len()
    );
    // The throw-away half of the canary: a path exclusion that has nothing to do with test code must
    // read as `false`, or the predicate is a constant in the other direction and CLAUSE D goes back to
    // reading mere presence. Written out here rather than read off a rule, so no bundled id is named.
    assert!(
        !zzop_core::dsl::declines_shared_test_paths("(?:^|/)locales?/[a-z]{2,3}(?:/|\\.json$)"),
        "a path exclusion naming no test convention reads as a test-path exclusion — the predicate is \
         answering `true` to everything and CLAUSE D is now failing rules it was narrowed to admit"
    );
}

/// The census clause every set-difference guard needs: all three clauses above compare sets, and the
/// difference of two empty sets is empty. A pack reader that collapsed to zero flagged rules would read
/// as perfect agreement, so the count is asserted to be nonzero — the number itself is deliberately NOT
/// pinned (that would be the hand list this file exists to avoid).
#[test]
fn at_least_one_bundled_rule_actually_carries_the_flag() {
    let flagged = bundled_rules()
        .values()
        .filter(|r| r.scan_test_regions)
        .count();
    assert!(
        flagged > 0,
        "no bundled rule sets `scan_test_regions` — either the credential family lost the opt-out and \
         committed keys inside `#[cfg(test)]` are being deleted again, or this guard stopped reading \
         the packs. Both read as green on the clauses above."
    );
}
