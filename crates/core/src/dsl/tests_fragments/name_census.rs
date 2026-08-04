//! The fragment-NAME census: every `${NAME}` fragment that ships is listed below, so a new one cannot
//! appear without a human triage moment against the policy-value inventory — the same moment
//! `scripts/check-policy-census.sh` forces for a new policy-shaped Rust `const`. A fragment is the
//! structural TWIN of such a const: it has a stable name, it is referenced BY that name from other
//! sites, and one edit therefore moves several rules at once.
//!
//! ANONYMOUS inline pattern values (`line_pattern`, `exclude_pattern`, `patterns[].pattern`, …) are not
//! this module's subject, and since 2026-08-02 they are no longer nobody's: `crate::dsl::inline_census_tests`
//! censuses them into `scripts/dsl-inline-census.txt`. Until then this header said they "stay out for
//! the same reason a string literal spelled inline in an expression is out of the Rust half", and that
//! analogy was doing load-bearing work it could not do — `sql/nplus1` shipped a root-anchored
//! `file_pattern` that made a flagship rule silent under `src/api/`, with no triage record in any
//! census, because no census had a row for a value with no name. The two now partition the surface: a
//! value that IS a `${NAME}` reference is triaged here, once, and the inline census records it as axis
//! `named` at each use site rather than copying its content.
//!
//! ## Why this axis lives here and not in the census shell guard (moved 2026-07-25)
//! It was added to `scripts/check-policy-census.sh` first, as a line-oriented `awk` extractor over
//! `rules/dsl/**/*.json`, whose header asserted the line orientation "fails LOUD, never silent". That is
//! false. Measured against the shipped extractor, with inputs that are all VALID JSON and with no JSON
//! formatter guard anywhere in this repo to prevent them (no guard in `scripts/` formats JSON):
//!
//! | pack shape | expected | extractor produced |
//! |---|---|---|
//! | a second key appended to the line after `"fragments": {` | `alpha,beta,gamma` | `alpha,beta` — silent miss |
//! | the first key on the same line as the opening `{` | `alpha,beta` | `beta` — silent miss |
//! | the whole `fragments` map minified onto one line | `alpha,beta` | `rules` — a PHANTOM name, not the promised `removed:` |
//!
//! The first shape is the one that matters: a pack author appending `"new-veto": "..."` to an existing
//! line leaves the census output UNCHANGED, so the guard stays green and the triage moment is bypassed
//! at zero cost — which is the exact "structural evasion route" the JSON axis was introduced to close.
//!
//! ## Rejected: fix the `awk`
//! A correct extractor has to know where JSON strings begin and end (fragment VALUES are regexes, and
//! shipped ones already contain both `"` and `{`/`}` — see the values behind [`CENSUSED_FRAGMENTS`],
//! which is where that count lives so it cannot rot here), track brace depth, and tell a key
//! position from a value position — i.e. a hand-rolled JSON tokenizer, in `awk`, untested, ~30 lines
//! away from a real parser that this crate already runs over the very same files (`raw_packs`,
//! `fragments::shared_fragments`). Reading JSON with a better regex is still reading JSON with a regex;
//! the subtraction is to delete the second parser, not to improve it.
//!
//! ## Rejected: keep the `awk` and have this module assert its output is complete
//! That keeps two extractors for one fact and makes the known-wrong one authoritative for
//! `--update` — the pin would simply be red until the `awk` was fixed anyway, so it buys nothing the
//! rewrite does not.
//!
//! ## Rejected: a committed snapshot file with its own `--update` mode
//! The Rust half of the census earns its snapshot file: two orders of magnitude more rows, regenerated
//! mechanically (`scripts/policy-census.txt` is its own count). This axis is [`CENSUSED_FRAGMENTS`]
//! below — deliberately-bounded shared vocabulary you can read in one screen. An inline list needs no
//! second update command and
//! shows up in the diff of the change that adds the fragment, which is where the triage decision is
//! being made.

use std::collections::BTreeSet;

use super::super::fragments::shared_fragments;
use super::{raw_packs, repo_rel};

/// Path of the shared bundle, spelled the way the repo does. It lives OUTSIDE `rules/dsl` on purpose
/// (see `fragments.rs`'s header), so it is not reachable through `raw_packs`.
const SHARED_BUNDLE: &str = "crates/core/src/dsl/shared_fragments.json";

/// Every `${NAME}` fragment that ships today, as `<path>:<name>`, sorted.
///
/// Adding a row here is the triage moment: decide the tier (T1 shared / T2 / T3 / not-policy) and record
/// that verdict where it can be checked — a T1/T2 gets a pin in `zzop-engine`'s
/// `tests/rule_contracts/policy_pins.rs`, a T3 gets a "why these need not stay equal" line at the
/// declaration. (It used to say "record it in the policy-value inventory FIRST"; that table was folded
/// on 2026-07-26 in favour of code + mechanical cross-check, so the instruction named a dead structure.)
/// Removing a row is equally deliberate — a fragment that disappears was referenced by name from
/// somewhere.
const CENSUSED_FRAGMENTS: &[&str] = &[
    "crates/core/src/dsl/shared_fragments.json:test-paths",
    "crates/core/src/dsl/shared_fragments.json:test-paths-migrations",
    "crates/core/src/dsl/shared_fragments.json:test-paths-stories",
    "rules/dsl/browser/browser.json:html-sink-sanitized",
    "rules/dsl/redis/redis.json:string-denylist-literal",
    "rules/dsl/sql/sql.json:sql-bootstrap-drop-create",
    "rules/dsl/sql/sql.json:sql-where-veto",
];

/// Every fragment name the SHIPPED tree actually defines, read through `serde_json` — the shared bundle
/// plus every pack's own raw `fragments` map. Never a text scan: that is the whole point of the move.
fn shipped_fragment_names() -> BTreeSet<String> {
    let mut out: BTreeSet<String> = shared_fragments()
        .keys()
        .map(|name| format!("{SHARED_BUNDLE}:{name}"))
        .collect();
    for (path, pack) in raw_packs() {
        let rel = repo_rel(&path);
        out.extend(pack.fragments.keys().map(|name| format!("{rel}:{name}")));
    }
    out
}

/// Policy pin: the shipped fragment names and [`CENSUSED_FRAGMENTS`] are the same set, in both
/// directions. `added` is the triage moment this census exists for; `removed` is the drift signal the
/// shell census reports under the same name.
#[test]
fn every_shipped_fragment_name_is_censused() {
    let shipped = shipped_fragment_names();
    let censused: BTreeSet<String> = CENSUSED_FRAGMENTS.iter().map(|s| (*s).to_owned()).collect();

    assert_eq!(
        CENSUSED_FRAGMENTS.len(),
        censused.len(),
        "CENSUSED_FRAGMENTS contains a duplicate row"
    );
    assert!(
        !shipped.is_empty(),
        "no `${{NAME}}` fragment found in the shared bundle or in rules/dsl/** — this pin would pass \
         vacuously; if the mechanism really was removed, remove this census with it"
    );

    let added: Vec<&String> = shipped.difference(&censused).collect();
    let removed: Vec<&String> = censused.difference(&shipped).collect();
    assert!(
        added.is_empty() && removed.is_empty(),
        "the shipped `${{NAME}}` fragment vocabulary has drifted from CENSUSED_FRAGMENTS.\n  \
         added:   {added:?}\n  removed: {removed:?}\n\
         A new named fragment is shared, referenced vocabulary — triage it against the policy-value \
         inventory (tier T1/T2/T3, or not-policy) and then add its row above. A removed one means a \
         name other sites referenced is gone."
    );
}

/// Pin: [`CENSUSED_FRAGMENTS`] stays sorted, so a new row lands next to its siblings and a diff shows one
/// added line rather than a reshuffle. Same reason `scripts/policy-census.txt` is `sort -u`'d.
#[test]
fn the_censused_fragment_list_is_sorted() {
    let mut sorted = CENSUSED_FRAGMENTS.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        CENSUSED_FRAGMENTS,
        &sorted[..],
        "CENSUSED_FRAGMENTS is not in sorted order"
    );
}
