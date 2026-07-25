//! Seals the one relationship the fragment mechanism cannot express itself: a fragment that EXTENDS the
//! shared `test-paths` vocabulary must still CONTAIN all of it.
//!
//! ## Why a reference can't carry this, so a test must
//! `${NAME}` is a WHOLE-VALUE reference by design: `"${test-paths}|<extra arm>"` is a hard load error, not
//! a concatenation (pinned by `expansion_tests::expand_fragments_errs_on_a_nested_fragment_reference_
//! rather_than_chaining`). So a fragment that wants "everything `test-paths` excludes, plus one more arm"
//! has no choice but to HAND-COPY the shared body — the exact duplication the fragment mechanism exists to
//! abolish, reappearing inside the mechanism. Two such copies ship today: `test-paths-stories` (in the
//! shared bundle itself) and `sql`'s `test-paths-migrations`. Nothing else notices when the shared body
//! gains an arm and the copies silently keep the old one: each pack's fixtures only exercise its own
//! `file_exclude_pattern`, and a MISSING exclusion buys extra findings on test files nobody attributes to
//! a stale copy.
//!
//! ## The criterion: top-level alternation ARM-SET inclusion, not string prefix
//! Both copies happen to be `(?i)(A|B|C)` with extra arms added, so "the shared body minus its closing
//! paren is a PREFIX of the copy" looks like a cheaper test. It is measurably wrong: `test-paths-stories`
//! inserts `\.stories\.` and `(^|/)\.storybook/` in the MIDDLE of the shared arms, so the prefix criterion
//! fails on a copy that is a perfectly good superset — a false red on day one, for a difference
//! (alternation ORDER) that cannot change behavior, since these values are only ever asked `is_match`.
//! Arm-set inclusion is exactly the property that matters for a boolean match: every path the base
//! excludes, the extension excludes too. The inline flag group (`(?i)`) is asserted separately — equal
//! arms with a lost `(?i)` would still diverge.
//!
//! Residual, stated plainly: a hand-copy under a name OUTSIDE the `test-paths-*` family escapes this pin.
//! That half is the sibling `name_census` module's job — it censuses every fragment NAME in the shared
//! bundle and in `rules/dsl/**`, so a new fragment cannot appear without a human triage moment, and this
//! pin is what that moment is supposed to reach for. (That census lived in
//! `scripts/check-policy-census.sh` until 2026-07-25; see `name_census`'s header for the measured reason
//! a line-oriented shell extractor could not hold it.)

use std::collections::BTreeSet;

use super::super::fragments::shared_fragments;
use super::{raw_packs, repo_rel};

/// The shared fragment every `{BASE}-*` extension must contain in full.
const BASE: &str = "test-paths";

/// A fragment read as `FLAGS( ARM | ARM | … )` — the only shape this pin can reason about.
struct Alternation {
    /// The leading inline-flag group (`(?i)`), or empty if the value has none.
    flags: String,
    /// The members of the single top-level alternation group, as a set: order is meaningless to a
    /// boolean `is_match`, so pinning it would only manufacture false reds.
    arms: BTreeSet<String>,
}

/// Splits `value` into its inline flags and its top-level alternation arms, respecting nesting,
/// backslash escapes and `[...]` classes so an arm is never cut at a `|` that belongs to a subgroup or a
/// character class. Panics if `value` is not `FLAGS?( … )` with the group spanning the whole remainder:
/// the shape this pin reads its vocabulary out of having changed is precisely the moment a human should
/// look, not a moment to silently pin nothing (same contract as `policy_pins`' alternation reader in
/// `crates/engine/tests/rule_contracts`).
fn parse_alternation(origin: &str, value: &str) -> Alternation {
    let flag_len = value
        .strip_prefix("(?")
        .and_then(|rest| rest.find(')').map(|end| (rest, end)))
        .filter(|(rest, end)| {
            *end > 0
                && rest[..*end]
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || c == '-')
        })
        .map_or(0, |(_, end)| "(?".len() + end + 1);
    let (flags, body) = value.split_at(flag_len);

    assert!(
        body.starts_with('(') && body.ends_with(')') && body.len() > 2,
        "{origin}: expected `FLAGS?( arm | arm | … )`, got {value:?} — this pin compares the members of \
         that single top-level group, so a changed shape must fail loudly instead of pinning nothing"
    );

    let mut arms = BTreeSet::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_class = false;
    let mut escaped = false;
    for c in body[1..body.len() - 1].chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            _ if in_class => {}
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                assert!(
                    depth >= 0,
                    "{origin}: the leading `(` in {value:?} closes before the end of the value, so the \
                     value is not one alternation group this pin can split"
                );
            }
            '|' if depth == 0 => {
                arms.insert(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    assert!(
        depth == 0 && !in_class && !escaped,
        "{origin}: unbalanced parens/class/escape in {value:?}"
    );
    arms.insert(current);

    Alternation {
        flags: flags.to_owned(),
        arms,
    }
}

/// Every shipped fragment named `test-paths-*`, as `(origin, value)` read out of what actually ships —
/// the shared bundle plus every pack's own raw `fragments` map. Never a hand-copied list here: a third
/// mirror of the vocabulary is exactly the failure this pin exists to catch.
fn test_paths_extensions() -> Vec<(String, String)> {
    let prefix = format!("{BASE}-");
    let mut out: Vec<(String, String)> = shared_fragments()
        .iter()
        .filter(|(name, _)| name.starts_with(&prefix))
        .map(|(name, value)| {
            (
                format!("crates/core/src/dsl/shared_fragments.json:{name}"),
                value.clone(),
            )
        })
        .collect();

    for (path, pack) in raw_packs() {
        for (name, value) in &pack.fragments {
            if name.starts_with(&prefix) {
                out.push((format!("{}:{name}", repo_rel(&path)), value.clone()));
            }
        }
    }
    out.sort();
    out
}

/// Policy pin: every `test-paths-*` fragment is a hand-copy of the shared `test-paths` body plus extra
/// alternation arms (the mechanism forbids saying so by reference — see this module's doc), so each one
/// must remain a strict SUPERSET of it: same inline flags, every base arm still present, and at least one
/// arm of its own. Both sides are read from the shipped bytes, so the pin cannot drift from the vocabulary
/// it guards.
///
/// The "at least one arm of its own" half is not decoration: a copy that has shrunk back to exactly the
/// base has no reason to exist as a separate fragment, and the correct fix is to delete it and reference
/// `${test-paths}` — which the mechanism DOES support. Failing there routes that edit to the subtraction.
#[test]
fn every_test_paths_extension_is_a_strict_superset_of_the_shared_test_paths_fragment() {
    let base_value = shared_fragments().get(BASE).unwrap_or_else(|| {
        panic!("shared bundle must define `{BASE}` — every extension copies it")
    });
    let base = parse_alternation(&format!("shared:{BASE}"), base_value);
    assert!(
        base.arms.len() >= 2,
        "`{BASE}` split into {} arm(s) — the splitter degenerated, and a one-arm base would make the \
         inclusion assertion below near-vacuous",
        base.arms.len()
    );

    let extensions = test_paths_extensions();
    assert!(
        !extensions.is_empty(),
        "no `{BASE}-*` fragment found in the shared bundle or in rules/dsl/** — this pin would pass \
         vacuously; if the last extension really was removed, remove this pin with it"
    );

    for (origin, value) in &extensions {
        let extension = parse_alternation(origin, value);
        assert_eq!(
            extension.flags, base.flags,
            "{origin} carries inline flags {:?} while `{BASE}` carries {:?} — same arms under different \
             flags still exclude different files",
            extension.flags, base.flags
        );

        let missing: Vec<&String> = base.arms.difference(&extension.arms).collect();
        assert!(
            missing.is_empty(),
            "{origin} is a hand-copy of the shared `{BASE}` body plus extra arms, and it has drifted: it \
             is missing {missing:?}. Copy the shared arms across (a `${{{BASE}}}` reference cannot express \
             \"base plus one arm\" — nested references are a hard load error). Until then every file the \
             base excludes but this copy does not is scanned by the rules that reference it."
        );

        let extra: Vec<&String> = extension.arms.difference(&base.arms).collect();
        assert!(
            !extra.is_empty(),
            "{origin} now has exactly the arms of `{BASE}` and so has no reason to exist — replace its \
             uses with `${{{BASE}}}` and delete it"
        );
    }
}
